//! Der Import schickt historische Nachrichten mit Originalzeitpunkt durch den normalen Weg.

use std::io::Cursor;
use std::sync::Arc;

use ingestion_service::import::{Stats, import_lines};
use ingestion_service::observer::Dispatcher;
use ingestion_service::store::PostgresObserver;
use shared::Device;
use shared::db::MIGRATOR;
use sqlx::PgPool;

fn dispatcher(pool: &PgPool) -> Dispatcher {
    let mut d = Dispatcher::new();
    d.subscribe(Arc::new(PostgresObserver::new(pool.clone())));
    d
}

fn line(t: &str, topic: &str, payload: &str) -> String {
    format!(r#"{{"t":"{t}","topic":"{topic}","payload":{payload}}}"#)
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn replays_with_original_time_and_integrates_the_current(pool: PgPool) {
    // 8 A über genau eine Stunde sind 8 Ah, also 10 Prozentpunkte von 80 Ah. Start bei voller Batterie, Messung alle
    // 15 Minuten wie bei den Daten der Messdatenplattform. Die Spannungen folgen der Kurve (100 bis 90 Prozent).
    let input = [
        ("10:00", 12_594),
        ("10:15", 12_566),
        ("10:30", 12_528),
        ("10:45", 12_491),
        ("11:00", 12_448),
    ]
    .map(|(time, voltage)| {
        line(
            &format!("2026-10-14T{time}:00Z"),
            "traffic-nodes/TN0001/measurements",
            &format!(r#"{{"current_mA":-8000,"voltage_mV":{voltage},"temperature_C":15}}"#),
        )
    })
    .into_iter()
    .chain([String::new()])
    .collect::<Vec<_>>()
    .join("\n");

    let stats = import_lines(&dispatcher(&pool), Cursor::new(input)).await.unwrap();

    assert_eq!(stats, Stats { valid: 5, invalid: 0 });
    let device: Device = sqlx::query_as("select * from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!((device.discharged_ah - 8.0).abs() < 1e-6);
    assert!((86.0..94.0).contains(&device.soc_percent.unwrap()));
    assert_eq!(
        device.last_seen_at.unwrap().to_rfc3339(),
        "2026-10-14T11:00:00+00:00",
        "Originalzeitpunkt, nicht die Importzeit"
    );
    let times: Vec<String> =
        sqlx::query_scalar("select to_char(time at time zone 'utc', 'YYYY-MM-DD HH24:MI') from readings order by time")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        times,
        [
            "2026-10-14 10:00",
            "2026-10-14 10:15",
            "2026-10-14 10:30",
            "2026-10-14 10:45",
            "2026-10-14 11:00"
        ]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invalid_messages_are_logged_not_stored(pool: PgPool) {
    let input = [
        line(
            "2026-10-14T10:00:00Z",
            "traffic-nodes/101/measurements",
            r#"{"current_mA":-5,"voltage_mV":5368709120,"temperature_C":15}"#,
        ),
        line(
            "2026-10-14T10:05:00Z",
            "traffic-nodes/101/measurements",
            r#""kein json {""#,
        ),
        line(
            "2026-10-14T10:10:00Z",
            "traffic-nodes/101/measurements",
            r#"{"current_mA":-5,"voltage_mV":12000,"temperature_C":15}"#,
        ),
    ]
    .join("\n");

    let stats = import_lines(&dispatcher(&pool), Cursor::new(input)).await.unwrap();

    assert_eq!(stats, Stats { valid: 1, invalid: 2 });
    let logged: i64 = sqlx::query_scalar("select count(*) from invalid_messages_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(logged, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_broken_line_aborts_with_its_line_number(pool: PgPool) {
    let input = format!(
        "{}\nkaputt\n",
        line(
            "2026-10-14T10:00:00Z",
            "traffic-nodes/TN0001/measurements",
            r#"{"current_mA":-5,"voltage_mV":12000,"temperature_C":15}"#
        )
    );
    let error = import_lines(&dispatcher(&pool), Cursor::new(input)).await.unwrap_err();
    assert!(format!("{error:#}").contains("Zeile 2"), "{error:#}");
}
