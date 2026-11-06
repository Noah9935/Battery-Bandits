//! Durchstich: echte Nachrichten über den Mosquitto der Dev-Umgebung bis in die Datenbank.
//! Benötigt DATABASE_URL und MQTT_HOST (im Dev-Container gesetzt).

use std::sync::Arc;
use std::time::Duration;

use ingestion_service::config::Config;
use ingestion_service::observer::Dispatcher;
use ingestion_service::{mqtt, store::PostgresObserver};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use shared::db::MIGRATOR;
use sqlx::PgPool;

async fn wait_for<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("Zeitüberschreitung beim Warten auf: {what}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn messages_travel_from_broker_to_database(pool: PgPool) {
    let host = std::env::var("MQTT_HOST").unwrap_or_else(|_| "localhost".into());
    let port: u16 = std::env::var("MQTT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1883);
    let suffix = format!(
        "{:x}",
        std::process::id() ^ (std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u32)
    );
    let node = format!("IT{suffix}");

    // Nachrichten als "retained" senden: Der Empfänger bekommt sie beim Abonnieren, ohne Wettlauf.
    let (client, mut eventloop) = AsyncClient::new(MqttOptions::new(format!("it-pub-{suffix}"), &host, port), 16);
    let good_topic = format!("traffic-nodes/{node}/measurements");
    let bad_topic = format!("traffic-nodes/{node}bad/measurements");
    let good = br#"{"current_mA":-311.25,"voltage_mV":12296.25,"lat":null,"lon":null,"altitude_m":null,
        "satellites":null,"time":null,"temperature_C":16.44}"#;
    client
        .publish(&good_topic, QoS::AtLeastOnce, true, good.as_slice())
        .await
        .unwrap();
    client
        .publish(&bad_topic, QoS::AtLeastOnce, true, "das ist kein json")
        .await
        .unwrap();
    tokio::spawn(async move { while eventloop.poll().await.is_ok() {} });

    let config = Config {
        database_url: String::new(),
        mqtt_host: host,
        mqtt_port: port,
        mqtt_user: None,
        mqtt_password: None,
        mqtt_tls: false,
        mqtt_topic_filter: "traffic-nodes/+/measurements".into(),
        mqtt_client_id: format!("it-sub-{suffix}"),
        data_source: shared::DataSource::Simulation,
    };

    let mut dispatcher = Dispatcher::new();
    dispatcher.subscribe(Arc::new(PostgresObserver::new(pool.clone())));
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let service = tokio::spawn(async move {
        mqtt::run(&config, dispatcher, async {
            let _ = stop_rx.await;
        })
        .await;
    });

    let p = pool.clone();
    let n = node.clone();
    wait_for("gültige Nachricht in readings", || {
        let (p, n) = (p.clone(), n.clone());
        async move {
            sqlx::query_scalar::<_, i64>("select count(*) from readings where device_id = $1")
                .bind(n)
                .fetch_one(&p)
                .await
                .unwrap()
                == 1
        }
    })
    .await;
    let p = pool.clone();
    wait_for("ungültige Nachricht im Protokoll", || {
        let p = p.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "select count(*) from invalid_messages_log where topic like '%bad/measurements'",
            )
            .fetch_one(&p)
            .await
            .unwrap()
                >= 1
        }
    })
    .await;

    let (voltage, soc): (i32, f32) = sqlx::query_as("select voltage_mv, soc_percent from devices where id = $1")
        .bind(&node)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(voltage, 12_296);
    assert!(soc > 80.0 && soc < 83.0, "SoC aus Spannung: {soc}");

    // Aufräumen: Retained-Nachrichten löschen, Empfänger stoppen.
    client
        .publish(&good_topic, QoS::AtLeastOnce, true, Vec::<u8>::new())
        .await
        .unwrap();
    client
        .publish(&bad_topic, QoS::AtLeastOnce, true, Vec::<u8>::new())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = stop_tx.send(());
    tokio::time::timeout(Duration::from_secs(5), service)
        .await
        .unwrap()
        .unwrap();
}
