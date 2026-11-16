//! Merkliste und Touren gehören dem Konto, Touren lassen sich teilen. Gegen eine echte Datenbank.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use operations_service::http::router;
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use sqlx::PgPool;
use tower::ServiceExt;

async fn call(app: &Router, user: Option<&str>, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(user) = user {
        request = request
            .header("x-bb-user", user)
            .header("x-bb-name", user.to_uppercase());
    }
    let body = match body {
        Some(v) => {
            request = request.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn seed(pool: &PgPool) {
    sqlx::query(
        "insert into users (username, display_name, password_hash) values \
         ('noah', 'Noah Boufercha', 'x'), ('fynn', 'Fynn Becker', 'x'), ('lena', 'Lena Test', 'x')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("insert into devices (id, status) values ('A', 'normal'), ('B', 'normal'), ('C', 'normal')")
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_account_has_its_own_pin_list(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool);
    let noah = Some("noah");
    let fynn = Some("fynn");

    assert_eq!(
        call(&app, noah, Method::PUT, "/pins/A", Some(json!({}))).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, noah, Method::PUT, "/pins/B", Some(json!({}))).await.0,
        StatusCode::OK
    );
    // Doppelt merken schadet nicht, ein unbekannter Node wird übergangen.
    assert_eq!(
        call(&app, noah, Method::PUT, "/pins/A", Some(json!({}))).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, noah, Method::PUT, "/pins/GIBTS-NICHT", Some(json!({})))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(call(&app, noah, Method::GET, "/pins", None).await.1, json!(["A", "B"]));
    // Fynn sieht davon nichts.
    assert_eq!(call(&app, fynn, Method::GET, "/pins", None).await.1, json!([]));

    let added = call(
        &app,
        fynn,
        Method::POST,
        "/pins",
        Some(json!({"ids": ["C", " A ", "A", ""]})),
    )
    .await;
    assert_eq!(added.1, json!(["A", "C"]));

    assert_eq!(
        call(&app, noah, Method::DELETE, "/pins/A", None).await.0,
        StatusCode::OK
    );
    assert_eq!(call(&app, noah, Method::GET, "/pins", None).await.1, json!(["B"]));
    assert_eq!(call(&app, noah, Method::DELETE, "/pins", None).await.0, StatusCode::OK);
    assert_eq!(call(&app, noah, Method::GET, "/pins", None).await.1, json!([]));
    assert_eq!(
        call(&app, fynn, Method::GET, "/pins", None)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        2,
        "Fynns Liste bleibt"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pins_and_tours_need_a_login(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool);
    assert_eq!(call(&app, None, Method::GET, "/pins", None).await.1, json!([]));
    assert_eq!(
        call(&app, None, Method::PUT, "/pins/A", Some(json!({}))).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(call(&app, None, Method::GET, "/tours", None).await.1, json!([]));
    assert_eq!(
        call(&app, None, Method::POST, "/tours", Some(json!({"name": "X"})))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_tour_can_be_shared_and_walked_but_only_the_owner_changes_it(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool);
    let (noah, fynn, lena) = (Some("noah"), Some("fynn"), Some("lena"));

    let (status, tour) = call(
        &app,
        noah,
        Method::POST,
        "/tours",
        Some(json!({"name": "  Dienstag Nord ", "device_ids": ["A", "B", "A"], "share_with": ["fynn", "noah"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{tour}");
    let id = tour["id"].as_i64().unwrap();
    assert_eq!(tour["name"], "Dienstag Nord");
    assert_eq!(tour["device_ids"], json!(["A", "B"]));
    assert_eq!(tour["shared_with"], json!(["fynn"]), "man teilt nicht mit sich selbst");
    assert_eq!(tour["mine"], true);

    // Fynn sieht sie als geteilt, ohne die Liste der anderen Empfänger.
    let (_, seen) = call(&app, fynn, Method::GET, "/tours", None).await;
    assert_eq!(seen.as_array().unwrap().len(), 1);
    assert_eq!(seen[0]["mine"], false);
    assert_eq!(seen[0]["owner"], "noah");
    assert_eq!(seen[0]["owner_name"], "Noah Boufercha");
    assert_eq!(seen[0]["shared_with"], json!([]));
    assert_eq!(seen[0]["device_ids"], json!(["A", "B"]));
    // Lena sieht nichts.
    assert_eq!(call(&app, lena, Method::GET, "/tours", None).await.1, json!([]));
    assert_eq!(
        call(
            &app,
            lena,
            Method::PUT,
            &format!("/tours/{id}"),
            Some(json!({"name": "x"}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // Fynn bekommt einen Hinweis im Postfach.
    let (_, inbox) = call(&app, fynn, Method::GET, "/inbox", None).await;
    let hint = inbox
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "shared")
        .expect("Hinweis fehlt");
    assert!(hint["text"].as_str().unwrap().contains("Dienstag Nord"), "{hint}");

    // Ändern dürfen alle, die die Tour sehen. Teilen und löschen darf nur die Besitzerin oder der Besitzer.
    let uri = format!("/tours/{id}");
    assert_eq!(
        call(&app, fynn, Method::PUT, &uri, Some(json!({"name": "Meins"})))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            fynn,
            Method::PUT,
            &format!("{uri}/shares"),
            Some(json!({"usernames": ["lena"]}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&app, fynn, Method::DELETE, &uri, None).await.0,
        StatusCode::FORBIDDEN
    );

    // Die Besitzerin ändert die Nodes, Fynn sieht die neue Reihenfolge.
    let (status, changed) = call(&app, noah, Method::PUT, &uri, Some(json!({"device_ids": ["C", "A"]}))).await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        call(&app, fynn, Method::GET, "/tours", None).await.1[0]["device_ids"],
        json!(["C", "A"])
    );

    // Freigabe ersetzen: Lena kommt dazu, Fynn bleibt, nur Lena bekommt einen neuen Hinweis.
    let (status, shared_with) = call(
        &app,
        noah,
        Method::PUT,
        &format!("{uri}/shares"),
        Some(json!({"usernames": ["fynn", "lena"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{shared_with}");
    assert_eq!(shared_with["shared_with"], json!(["fynn", "lena"]));
    let fynn_hints = call(&app, fynn, Method::GET, "/inbox", None).await.1;
    assert_eq!(
        fynn_hints
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["kind"] == "shared")
            .count(),
        1
    );
    let lena_hints = call(&app, lena, Method::GET, "/inbox", None).await.1;
    assert_eq!(
        lena_hints
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["kind"] == "shared")
            .count(),
        1
    );

    // Unbekannte Personen werden abgelehnt.
    assert_eq!(
        call(
            &app,
            noah,
            Method::PUT,
            &format!("{uri}/shares"),
            Some(json!({"usernames": ["niemand"]}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Fynn nimmt die Tour aus seiner Liste, sie bleibt bei den anderen bestehen.
    assert_eq!(
        call(&app, fynn, Method::DELETE, &format!("{uri}/shares"), None).await.0,
        StatusCode::OK
    );
    assert_eq!(call(&app, fynn, Method::GET, "/tours", None).await.1, json!([]));
    assert_eq!(
        call(&app, lena, Method::GET, "/tours", None)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&app, fynn, Method::DELETE, &format!("{uri}/shares"), None).await.0,
        StatusCode::NOT_FOUND
    );

    // Löscht die Besitzerin die Tour, verschwindet sie für alle.
    assert_eq!(call(&app, noah, Method::DELETE, &uri, None).await.0, StatusCode::OK);
    assert_eq!(call(&app, lena, Method::GET, "/tours", None).await.1, json!([]));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tour_names_are_unique_per_person_and_checked(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool);
    let (noah, fynn) = (Some("noah"), Some("fynn"));
    assert_eq!(
        call(&app, noah, Method::POST, "/tours", Some(json!({"name": "Süd"})))
            .await
            .0,
        StatusCode::CREATED
    );
    // Derselbe Name bei derselben Person: Konflikt (auch mit anderer Schreibweise).
    assert_eq!(
        call(&app, noah, Method::POST, "/tours", Some(json!({"name": " süd "})))
            .await
            .0,
        StatusCode::CONFLICT
    );
    // Eine andere Person darf ihn nehmen.
    assert_eq!(
        call(&app, fynn, Method::POST, "/tours", Some(json!({"name": "Süd"})))
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(&app, noah, Method::POST, "/tours", Some(json!({"name": "  "})))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let long = "x".repeat(61);
    assert_eq!(
        call(&app, noah, Method::POST, "/tours", Some(json!({"name": long})))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn deleting_an_account_removes_its_pins_tours_and_shares(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    call(&app, Some("fynn"), Method::PUT, "/pins/A", Some(json!({}))).await;
    let (_, tour) = call(
        &app,
        Some("fynn"),
        Method::POST,
        "/tours",
        Some(json!({"name": "Fynns Tour", "device_ids": ["A"], "share_with": ["noah"]})),
    )
    .await;
    assert!(tour["id"].is_i64());
    call(
        &app,
        Some("noah"),
        Method::POST,
        "/tours",
        Some(json!({"name": "Noahs", "share_with": ["fynn"]})),
    )
    .await;

    // Wie beim Löschen des Kontos im Dashboard-Service.
    for table in ["user_pins", "tour_shares"] {
        sqlx::query(&format!("delete from {table} where username = 'fynn'"))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("delete from tours where owner = 'fynn'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, Some("noah"), Method::GET, "/tours", None)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let rest: i64 = sqlx::query_scalar("select count(*) from tour_shares")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rest, 0, "keine Freigabe zeigt mehr auf eine fremde oder gelöschte Tour");
}
