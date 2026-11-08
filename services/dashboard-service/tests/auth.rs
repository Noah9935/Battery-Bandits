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
            },
            NewUser {
                username: "fynn".into(),
                password: "geheim-fynn".into(),
                display_name: "Fynn Becker".into(),
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
        login.body,
        json!({"username": "noah", "display_name": "Noah Boufercha"})
    );
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
    assert_eq!(
        reply.body,
        json!([
            {"username": "fynn", "display_name": "Fynn Becker"},
            {"username": "noah", "display_name": "Noah Boufercha"}
        ])
    );
    assert!(!reply.body.to_string().contains("argon2"));
}
