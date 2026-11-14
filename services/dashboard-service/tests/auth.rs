//! Anmeldung: Zugriffsschutz, Login und Logout, Sperre nach Fehlversuchen, Name an den Operations-Service.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::Request as AxumRequest;
use axum::http::{Method, Request, StatusCode, header};
use axum::routing::any;
use dashboard_service::auth::{NewUser, upsert_users};
use dashboard_service::{AppState, api::router};
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use sqlx::PgPool;
use tower::ServiceExt;

struct Reply {
    status: StatusCode,
    cookie: Option<String>,
    body: Value,
}

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>, cookie: Option<&str>) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let body = match body {
        Some(v) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let (parts, body) = response.into_parts();
    Reply {
        status: parts.status,
        cookie: parts
            .headers
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().to_owned()),
        body: serde_json::from_slice(&to_bytes(body, 1 << 20).await.unwrap()).unwrap_or(Value::Null),
    }
}

async fn secured(pool: PgPool) -> Router {
    upsert_users(
        &pool,
        &[
            NewUser {
                username: "noah".into(),
                password: "geheim-noah".into(),
                display_name: "Noah Boufercha".into(),
                admin: false,
            },
            NewUser {
                username: "fynn".into(),
                password: "geheim-fynn".into(),
                display_name: "Fynn Becker".into(),
                admin: false,
            },
        ],
    )
    .await
    .unwrap();
    router(AppState::new(pool, "http://127.0.0.1:1").with_auth(true))
}

fn creds(user: &str, password: &str) -> Option<Value> {
    Some(json!({"username": user, "password": password}))
}

/// `bb_session=...` aus einer Set-Cookie-Zeile.
fn session_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_owned()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_a_session_only_health_and_login_are_reachable(pool: PgPool) {
    let app = secured(pool).await;
    for uri in [
        "/api/summary",
        "/api/devices",
        "/api/settings",
        "/api/events",
        "/api/alerts",
        "/api/auth/me",
        "/api/trends",
        "/api/users",
        "/api/activity",
    ] {
        let reply = call(&app, Method::GET, uri, None, None).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert_eq!(reply.body["error"], "Nicht angemeldet.");
    }
    assert_eq!(
        call(&app, Method::GET, "/healthz", None, None).await.status,
        StatusCode::OK
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn login_gives_a_session_that_opens_the_api_and_logout_closes_it(pool: PgPool) {
    let app = secured(pool).await;

    let login = call(
        &app,
        Method::POST,
        "/api/auth/login",
        creds("Noah", "geheim-noah"),
        None,
    )
    .await;
    assert_eq!(
        login.status,
        StatusCode::OK,
        "Benutzername ohne Beachtung der Schreibweise"
    );
    assert_eq!(
        (login.body["username"].as_str(), login.body["display_name"].as_str()),
        (Some("noah"), Some("Noah Boufercha"))
    );
    assert!(login.body["avatar"].is_u64(), "dazu das Profilbild");
    let set_cookie = login.cookie.expect("Cookie");
    assert!(
        set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Lax"),
        "{set_cookie}"
    );
    let cookie = session_pair(&set_cookie);

    let me = call(&app, Method::GET, "/api/auth/me", None, Some(&cookie)).await;
    assert_eq!(
        (me.status, me.body["display_name"].as_str()),
        (StatusCode::OK, Some("Noah Boufercha"))
    );
    assert_eq!(
        call(&app, Method::GET, "/api/summary", None, Some(&cookie))
            .await
            .status,
        StatusCode::OK
    );

    let logout = call(&app, Method::POST, "/api/auth/logout", None, Some(&cookie)).await;
    assert_eq!(logout.status, StatusCode::OK);
    assert!(logout.cookie.unwrap().contains("Max-Age=0"));
    assert_eq!(
        call(&app, Method::GET, "/api/auth/me", None, Some(&cookie))
            .await
            .status,
        StatusCode::UNAUTHORIZED,
        "Sitzung ist weg"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn wrong_credentials_are_rejected_without_a_cookie(pool: PgPool) {
    let app = secured(pool).await;
    for (user, password) in [("noah", "falsch"), ("niemand", "geheim-noah"), ("", "")] {
        let reply = call(&app, Method::POST, "/api/auth/login", creds(user, password), None).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{user}");
        assert!(reply.cookie.is_none());
        assert_eq!(reply.body["error"], "Benutzername oder Passwort falsch.");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_forged_or_expired_session_is_rejected(pool: PgPool) {
    let app = secured(pool.clone()).await;
    assert_eq!(
        call(&app, Method::GET, "/api/summary", None, Some("bb_session=erfunden"))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    let login = call(
        &app,
        Method::POST,
        "/api/auth/login",
        creds("noah", "geheim-noah"),
        None,
    )
    .await;
    let cookie = session_pair(&login.cookie.unwrap());
    sqlx::query("update sessions set expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, Method::GET, "/api/summary", None, Some(&cookie))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn five_wrong_passwords_lock_the_name_for_a_while(pool: PgPool) {
    let app = secured(pool).await;
    for _ in 0..5 {
        assert_eq!(
            call(&app, Method::POST, "/api/auth/login", creds("noah", "falsch"), None)
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }
    let locked = call(
        &app,
        Method::POST,
        "/api/auth/login",
        creds("noah", "geheim-noah"),
        None,
    )
    .await;
    assert_eq!(
        locked.status,
        StatusCode::TOO_MANY_REQUESTS,
        "auch das richtige Passwort zählt in der Sperre nicht"
    );
    let other = call(
        &app,
        Method::POST,
        "/api/auth/login",
        creds("fynn", "geheim-fynn"),
        None,
    )
    .await;
    assert_eq!(other.status, StatusCode::OK, "andere Konten sind nicht betroffen");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn passwords_are_stored_hashed_and_users_can_be_updated(pool: PgPool) {
    let app = secured(pool.clone()).await;
    let hash: String = sqlx::query_scalar("select password_hash from users where username = 'noah'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2") && !hash.contains("geheim-noah"));

    upsert_users(
        &pool,
        &[NewUser {
            username: "noah".into(),
            password: "neues-passwort".into(),
            display_name: "Noah B.".into(),
            admin: false,
        }],
    )
    .await
    .unwrap();
    assert_eq!(
        call(
            &app,
            Method::POST,
            "/api/auth/login",
            creds("noah", "geheim-noah"),
            None
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    let ok = call(
        &app,
        Method::POST,
        "/api/auth/login",
        creds("noah", "neues-passwort"),
        None,
    )
    .await;
    assert_eq!(ok.body["display_name"], "Noah B.");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_the_requirement_everyone_is_a_guest(pool: PgPool) {
    let app = router(AppState::new(pool, "http://127.0.0.1:1"));
    let me = call(&app, Method::GET, "/api/auth/me", None, None).await;
    assert_eq!(
        (me.status, me.body["username"].as_str()),
        (StatusCode::OK, Some("gast"))
    );
    assert_eq!(
        call(&app, Method::GET, "/api/summary", None, None).await.status,
        StatusCode::OK
    );
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_session_name_reaches_the_operations_service_and_forged_headers_do_not(pool: PgPool) {
    let upstream = serve(Router::new().fallback(any(|req: AxumRequest| async move {
        let h = |name: &str| req.headers().get(name).map(|v| v.to_str().unwrap().to_owned());
        axum::Json(json!({"user": h("x-bb-user"), "name": h("x-bb-name")}))
    })))
    .await;
    upsert_users(
        &pool,
        &[NewUser {
            username: "jörg".into(),
            password: "geheim-jörg".into(),
            display_name: "Jörg Müller".into(),
            admin: false,
        }],
    )
    .await
    .unwrap();
    let dashboard = serve(router(AppState::new(pool, upstream).with_auth(true))).await;
    let client = reqwest::Client::new();

    let login = client
        .post(format!("{dashboard}/api/auth/login"))
        .json(&json!({"username": "jörg", "password": "geheim-jörg"}))
        .send()
        .await
        .unwrap();
    let cookie = session_pair(login.headers()[reqwest::header::SET_COOKIE].to_str().unwrap());

    let echoed: Value = client
        .get(format!("{dashboard}/api/alerts"))
        .header("cookie", &cookie)
        .header("x-bb-user", "admin")
        .header("x-bb-name", "Chef")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(
        echoed["user"], "j%C3%B6rg",
        "Name aus der Sitzung, kodiert, nicht der gefälschte"
    );
    assert_eq!(echoed["name"], "J%C3%B6rg%20M%C3%BCller");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_user_list_shows_names_but_no_secrets(pool: PgPool) {
    let app = secured(pool).await;
    let login = call(
        &app,
        Method::POST,
        "/api/auth/login",
        creds("noah", "geheim-noah"),
        None,
    )
    .await;
    let cookie = session_pair(&login.cookie.unwrap());

    let reply = call(&app, Method::GET, "/api/users", None, Some(&cookie)).await;

    assert_eq!(reply.status, StatusCode::OK);
    let people: Vec<(&str, &str)> = reply
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|u| (u["username"].as_str().unwrap(), u["display_name"].as_str().unwrap()))
        .collect();
    assert_eq!(people, [("fynn", "Fynn Becker"), ("noah", "Noah Boufercha")]);
    for user in reply.body.as_array().unwrap() {
        let keys: Vec<&String> = user.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            ["admin", "avatar", "display_name", "username"],
            "keine Geheimnisse in der Liste"
        );
    }
    assert!(!reply.body.to_string().contains("argon2"));
}

// ---------------------------------------------------------------- Angemeldet bleiben, Passwort, Profilbild

async fn login_as(app: &Router, user: &str, password: &str, remember: bool) -> Reply {
    call(
        app,
        Method::POST,
        "/api/auth/login",
        Some(json!({"username": user, "password": password, "remember": remember})),
        None,
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn remembering_the_login_keeps_the_session_for_over_a_year(pool: PgPool) {
    let app = secured(pool.clone()).await;

    let normal = login_as(&app, "noah", "geheim-noah", false).await;
    assert!(
        normal
            .cookie
            .as_deref()
            .unwrap()
            .contains(&format!("Max-Age={}", 14 * 24 * 3600))
    );
    let kept = login_as(&app, "noah", "geheim-noah", true).await;
    let cookie = kept.cookie.clone().unwrap();
    assert!(cookie.contains(&format!("Max-Age={}", 400 * 24 * 3600)), "{cookie}");
    assert!(cookie.contains("HttpOnly"));
    // Auch in der Datenbank läuft die Sitzung so lange.
    let days: f64 =
        sqlx::query_scalar("select (extract(epoch from max(expires_at) - now()) / 86400)::float8 from sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!((399.0..401.0).contains(&days), "{days}");
    assert_eq!(
        call(&app, Method::GET, "/api/auth/me", None, Some(&session_pair(&cookie)))
            .await
            .status,
        StatusCode::OK
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn everyone_gets_one_of_a_hundred_raccoons_and_keeps_it(pool: PgPool) {
    let app = secured(pool).await;
    let first = login_as(&app, "noah", "geheim-noah", false).await;
    let avatar = first.body["avatar"].as_u64().expect("Profilbild fehlt");
    assert!(avatar < 100);
    let again = login_as(&app, "noah", "geheim-noah", false).await;
    assert_eq!(again.body["avatar"].as_u64(), Some(avatar), "das Bild bleibt");
    let cookie = session_pair(&again.cookie.unwrap());
    let me = call(&app, Method::GET, "/api/auth/me", None, Some(&cookie)).await;
    assert_eq!(me.body["avatar"].as_u64(), Some(avatar));
    let users = call(&app, Method::GET, "/api/users", None, Some(&cookie)).await;
    assert!(
        users
            .body
            .as_array()
            .unwrap()
            .iter()
            .all(|u| u["avatar"].as_u64().is_some_and(|a| a < 100))
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_hundred_pictures_are_really_handed_out_at_random(pool: PgPool) {
    // 400 Konten: Mit dem Zufall landen sie auf deutlich mehr als einem Dutzend verschiedener Bilder.
    sqlx::query("insert into users (username, display_name, password_hash) select 'user' || g, 'Person ' || g, 'x' from generate_series(1, 400) g")
        .execute(&pool)
        .await
        .unwrap();
    let distinct: i64 = sqlx::query_scalar("select count(distinct avatar) from users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(distinct > 60, "{distinct} verschiedene Bilder bei 400 Konten");
    let outside: i64 = sqlx::query_scalar("select count(*) from users where avatar not between 0 and 99")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(outside, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn changing_the_password_needs_the_old_one_and_ends_other_sessions(pool: PgPool) {
    let app = secured(pool.clone()).await;
    let here = session_pair(&login_as(&app, "noah", "geheim-noah", false).await.cookie.unwrap());
    let elsewhere = session_pair(&login_as(&app, "noah", "geheim-noah", true).await.cookie.unwrap());
    let change = |current: &str, new: &str| {
        let body = json!({"current": current, "new": new});
        call(&app, Method::POST, "/api/auth/password", Some(body), Some(&here))
    };

    let wrong = change("falsch", "ein-neues-passwort").await;
    assert_eq!(
        (wrong.status, wrong.body["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("Das aktuelle Passwort stimmt nicht."))
    );
    let short = change("geheim-noah", "kurz").await;
    assert_eq!(short.status, StatusCode::UNPROCESSABLE_ENTITY);
    let same = change("geheim-noah", "geheim-noah").await;
    assert_eq!(same.status, StatusCode::UNPROCESSABLE_ENTITY);

    let ok = change("geheim-noah", "ein-neues-passwort").await;
    assert_eq!((ok.status, ok.body["ok"].as_bool()), (StatusCode::OK, Some(true)));

    assert_eq!(
        login_as(&app, "noah", "geheim-noah", false).await.status,
        StatusCode::UNAUTHORIZED,
        "das alte gilt nicht mehr"
    );
    assert_eq!(
        login_as(&app, "noah", "ein-neues-passwort", false).await.status,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, Method::GET, "/api/auth/me", None, Some(&here)).await.status,
        StatusCode::OK,
        "diese Sitzung bleibt"
    );
    assert_eq!(
        call(&app, Method::GET, "/api/auth/me", None, Some(&elsewhere))
            .await
            .status,
        StatusCode::UNAUTHORIZED,
        "das andere Gerät ist abgemeldet"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_configuration_does_not_overwrite_a_password_the_person_chose(pool: PgPool) {
    let app = secured(pool.clone()).await;
    let cookie = session_pair(&login_as(&app, "noah", "geheim-noah", false).await.cookie.unwrap());
    let body = json!({"current": "geheim-noah", "new": "eigenes-passwort"});
    assert_eq!(
        call(&app, Method::POST, "/api/auth/password", Some(body), Some(&cookie))
            .await
            .status,
        StatusCode::OK
    );

    // Neustart mit derselben Konfiguration, dazu ein neues Passwort für Fynn und ein geänderter Anzeigename.
    upsert_users(
        &pool,
        &[
            NewUser {
                username: "noah".into(),
                password: "geheim-noah".into(),
                display_name: "Noah B.".into(),
                admin: false,
            },
            NewUser {
                username: "fynn".into(),
                password: "fynn-neu-2026".into(),
                display_name: "Fynn Becker".into(),
                admin: false,
            },
        ],
    )
    .await
    .unwrap();

    assert_eq!(
        login_as(&app, "noah", "eigenes-passwort", false).await.status,
        StatusCode::OK,
        "Noahs eigenes bleibt"
    );
    assert_eq!(
        login_as(&app, "noah", "geheim-noah", false).await.status,
        StatusCode::UNAUTHORIZED
    );
    let noah = login_as(&app, "noah", "eigenes-passwort", false).await;
    assert_eq!(
        noah.body["display_name"], "Noah B.",
        "der Anzeigename folgt der Konfiguration"
    );
    assert_eq!(
        login_as(&app, "fynn", "fynn-neu-2026", false).await.status,
        StatusCode::OK,
        "Fynn hat nichts geändert"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_password_change_is_locked_after_repeated_wrong_attempts(pool: PgPool) {
    let app = secured(pool).await;
    let cookie = session_pair(&login_as(&app, "noah", "geheim-noah", false).await.cookie.unwrap());
    let attempt = |current: &str| {
        let body = json!({"current": current, "new": "ein-neues-passwort"});
        call(&app, Method::POST, "/api/auth/password", Some(body), Some(&cookie))
    };
    for _ in 0..5 {
        assert_eq!(attempt("falsch").await.status, StatusCode::FORBIDDEN);
    }
    assert_eq!(
        attempt("geheim-noah").await.status,
        StatusCode::TOO_MANY_REQUESTS,
        "auch das richtige wird jetzt gesperrt"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_a_session_the_password_cannot_be_changed(pool: PgPool) {
    let app = secured(pool).await;
    let body = json!({"current": "geheim-noah", "new": "ein-neues-passwort"});
    assert_eq!(
        call(&app, Method::POST, "/api/auth/password", Some(body), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

// ---------------------------------------------------------------- Admin-Rolle und Kontenverwaltung

async fn put_user(app: &Router, cookie: &str, who: &str, body: Value) -> Reply {
    call(
        app,
        Method::PUT,
        &format!("/api/admin/users/{who}"),
        Some(body),
        Some(cookie),
    )
    .await
}

async fn session_of(app: &Router, user: &str, password: &str) -> String {
    session_pair(&login_as(app, user, password, false).await.cookie.expect("Anmeldung"))
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_first_account_of_the_configuration_becomes_admin_when_there_is_none(pool: PgPool) {
    let app = secured(pool).await;
    let noah = login_as(&app, "noah", "geheim-noah", false).await;
    let fynn = login_as(&app, "fynn", "geheim-fynn", false).await;
    assert_eq!(
        (noah.body["admin"].as_bool(), fynn.body["admin"].as_bool()),
        (Some(true), Some(false))
    );
    let me = call(
        &app,
        Method::GET,
        "/api/auth/me",
        None,
        Some(&session_pair(&noah.cookie.unwrap())),
    )
    .await;
    assert_eq!(me.body["admin"], true);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_explicit_admin_in_the_configuration_wins(pool: PgPool) {
    upsert_users(
        &pool,
        &[
            NewUser {
                username: "noah".into(),
                password: "geheim-noah".into(),
                display_name: "Noah".into(),
                admin: false,
            },
            NewUser {
                username: "fynn".into(),
                password: "geheim-fynn".into(),
                display_name: "Fynn".into(),
                admin: true,
            },
        ],
    )
    .await
    .unwrap();
    let roles: Vec<(String, String)> = sqlx::query_as("select username, role from users order by username")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        roles,
        [
            ("fynn".to_owned(), "admin".to_owned()),
            ("noah".to_owned(), "user".to_owned())
        ]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_admins_may_manage_accounts(pool: PgPool) {
    let app = secured(pool).await;
    let fynn = session_of(&app, "fynn", "geheim-fynn").await;
    let new = json!({"username": "lena", "display_name": "Lena", "password": "lena-passwort"});
    for (method, uri, body) in [
        (Method::GET, "/api/admin/users", None),
        (Method::POST, "/api/admin/users", Some(new.clone())),
        (Method::PUT, "/api/admin/users/noah", Some(json!({"admin": false}))),
        (Method::DELETE, "/api/admin/users/noah", None),
    ] {
        let reply = call(&app, method, uri, body, Some(&fynn)).await;
        assert_eq!(
            (reply.status, reply.body["error"].as_str()),
            (StatusCode::FORBIDDEN, Some("Das dürfen nur Admins.")),
            "{uri}"
        );
    }
    assert_eq!(
        call(&app, Method::GET, "/api/admin/users", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_admin_creates_accounts_that_can_log_in(pool: PgPool) {
    let app = secured(pool).await;
    let noah = session_of(&app, "noah", "geheim-noah").await;
    let create = |body: Value| call(&app, Method::POST, "/api/admin/users", Some(body), Some(&noah));

    let ok = create(json!({"username": "lena", "display_name": " Lena Muster ", "password": "lena-passwort"})).await;
    assert_eq!(ok.status, StatusCode::CREATED);
    let lena = login_as(&app, "LENA", "lena-passwort", false).await;
    assert_eq!(
        lena.status,
        StatusCode::OK,
        "Benutzername ohne Beachtung der Schreibweise"
    );
    assert_eq!(
        (lena.body["display_name"].as_str(), lena.body["admin"].as_bool()),
        (Some("Lena Muster"), Some(false))
    );
    assert!(
        lena.body["avatar"].as_u64().is_some_and(|a| a < 100),
        "das Profilbild wird zufällig zugewiesen"
    );

    let taken = create(json!({"username": "lena", "display_name": "Zweite", "password": "lena-passwort"})).await;
    assert_eq!(
        (taken.status, taken.body["error"].as_str()),
        (StatusCode::CONFLICT, Some("Der Benutzername ist schon vergeben."))
    );
    for bad in [
        json!({"username": "x", "display_name": "X", "password": "lang-genug-1"}),
        json!({"username": "mit leerzeichen", "display_name": "X", "password": "lang-genug-1"}),
        json!({"username": "anna", "display_name": "  ", "password": "lang-genug-1"}),
        json!({"username": "anna", "display_name": "Anna", "password": "kurz"}),
    ] {
        assert_eq!(
            create(bad.clone()).await.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}"
        );
    }

    let boss =
        create(json!({"username": "chef", "display_name": "Chef", "password": "chef-passwort", "admin": true})).await;
    assert_eq!(boss.status, StatusCode::CREATED);
    assert_eq!(login_as(&app, "chef", "chef-passwort", false).await.body["admin"], true);

    let list = call(&app, Method::GET, "/api/admin/users", None, Some(&noah)).await;
    assert_eq!(list.status, StatusCode::OK);
    let names: Vec<(&str, bool)> = list
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["username"].as_str().unwrap(), a["admin"].as_bool().unwrap()))
        .collect();
    assert_eq!(
        names,
        [("chef", true), ("fynn", false), ("lena", false), ("noah", true)]
    );
    assert!(list.body[0]["created_at"].is_string() && list.body[0]["avatar"].is_u64());
    assert!(list.body[0].get("password_hash").is_none(), "keine Geheimnisse");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn roles_names_and_passwords_can_be_changed_but_the_last_admin_stays(pool: PgPool) {
    let app = secured(pool).await;
    let noah = session_of(&app, "noah", "geheim-noah").await;
    let fynn_session = session_of(&app, "fynn", "geheim-fynn").await;
    let update = |who: &'static str, body: Value| put_user(&app, &noah, who, body);

    // Der einzige Admin lässt sich nicht herabstufen.
    let last = update("noah", json!({"admin": false})).await;
    assert_eq!(
        (last.status, last.body["error"].as_str()),
        (StatusCode::CONFLICT, Some("Es muss mindestens ein Admin bleiben."))
    );

    assert_eq!(
        update("fynn", json!({"admin": true, "display_name": "Fynn B."}))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(login_as(&app, "fynn", "geheim-fynn", false).await.body["admin"], true);
    // Jetzt gibt es zwei Admins: Noah darf zum normalen Konto werden.
    assert_eq!(update("noah", json!({"admin": false})).await.status, StatusCode::OK);
    assert_eq!(
        call(&app, Method::GET, "/api/admin/users", None, Some(&noah))
            .await
            .status,
        StatusCode::FORBIDDEN,
        "Noah ist kein Admin mehr"
    );

    // Passwort zurücksetzen (jetzt durch Fynn): Das alte gilt nicht mehr, laufende Sitzungen enden.
    let fynn = session_of(&app, "fynn", "geheim-fynn").await;
    let reset = call(
        &app,
        Method::PUT,
        "/api/admin/users/noah",
        Some(json!({"password": "zurueck-gesetzt-1"})),
        Some(&fynn),
    )
    .await;
    assert_eq!(reset.status, StatusCode::OK);
    assert_eq!(
        login_as(&app, "noah", "geheim-noah", false).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login_as(&app, "noah", "zurueck-gesetzt-1", false).await.status,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, Method::GET, "/api/auth/me", None, Some(&noah)).await.status,
        StatusCode::UNAUTHORIZED,
        "alte Sitzung beendet"
    );
    assert_eq!(
        call(&app, Method::GET, "/api/auth/me", None, Some(&fynn_session))
            .await
            .status,
        StatusCode::OK
    );

    let short = call(
        &app,
        Method::PUT,
        "/api/admin/users/noah",
        Some(json!({"password": "kurz"})),
        Some(&fynn),
    )
    .await;
    assert_eq!(short.status, StatusCode::UNPROCESSABLE_ENTITY);
    let none = call(
        &app,
        Method::PUT,
        "/api/admin/users/gibtesnicht",
        Some(json!({"admin": true})),
        Some(&fynn),
    )
    .await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn deleting_an_account_removes_the_person_and_everything_that_belongs_to_them(pool: PgPool) {
    let app = secured(pool.clone()).await;
    let noah = session_of(&app, "noah", "geheim-noah").await;
    sqlx::query("insert into devices (id) values ('TN0001')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into node_subscriptions (username, device_id) values ('fynn', 'TN0001')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into inbox_items (username, kind, text) values ('fynn', 'alert', 'x')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into notification_rules (username, enabled) values ('fynn', true)")
        .execute(&pool)
        .await
        .unwrap();

    let own = call(&app, Method::DELETE, "/api/admin/users/noah", None, Some(&noah)).await;
    assert_eq!(
        (own.status, own.body["error"].as_str()),
        (StatusCode::CONFLICT, Some("Das eigene Konto lässt sich nicht löschen."))
    );
    assert_eq!(
        call(&app, Method::DELETE, "/api/admin/users/gibtesnicht", None, Some(&noah))
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        call(&app, Method::DELETE, "/api/admin/users/fynn", None, Some(&noah))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        login_as(&app, "fynn", "geheim-fynn", false).await.status,
        StatusCode::UNAUTHORIZED
    );
    for table in ["node_subscriptions", "inbox_items", "notification_rules"] {
        let left: i64 = sqlx::query_scalar(&format!("select count(*) from {table} where username = 'fynn'"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0, "{table}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_second_admin_can_come_and_go_but_the_last_one_stays(pool: PgPool) {
    let app = secured(pool).await;
    let noah = session_of(&app, "noah", "geheim-noah").await;
    let created = call(
        &app,
        Method::POST,
        "/api/admin/users",
        Some(json!({"username": "chef", "display_name": "Chef", "password": "chef-passwort", "admin": true})),
        Some(&noah),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(
        call(&app, Method::DELETE, "/api/admin/users/chef", None, Some(&noah))
            .await
            .status,
        StatusCode::OK
    );
    // Nur noch Noah ist Admin: Er lässt sich nicht herabstufen (siehe oben) und kann sich nicht selbst löschen.
    let last = call(
        &app,
        Method::PUT,
        "/api/admin/users/noah",
        Some(json!({"admin": false})),
        Some(&noah),
    )
    .await;
    assert_eq!(last.status, StatusCode::CONFLICT);
}
