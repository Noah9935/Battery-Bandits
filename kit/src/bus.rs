//! Ereignisbus auf Basis von NATS.
//!
//! * Ereignisse (`events.>`) laufen über JetStream: Sie werden gespeichert und
//!   jeder Dienst liest sie mit einem eigenen, dauerhaften Konsumenten. Ein
//!   Dienst, der kurz nicht läuft, verpasst nichts.
//! * Befehle (`cmd.>`) sind Request/Reply über Core NATS.

use std::future::Future;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_nats::jetstream::{self, AckKind, consumer::pull};
use futures_util::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::events::{CommandReply, subject};

use crate::retry::retry;

#[derive(Clone)]
pub struct Bus {
    client: async_nats::Client,
    js: jetstream::Context,
}

impl Bus {
    /// Verbindet mit NATS und stellt sicher, dass der Stream `EVENTS` existiert.
    pub async fn connect(url: &str) -> Result<Self> {
        let client = retry("NATS", 60, || async_nats::connect(url)).await?;
        let js = jetstream::new(client.clone());
        js.get_or_create_stream(jetstream::stream::Config {
            name: subject::STREAM.to_owned(),
            subjects: vec![subject::ALL_EVENTS.to_owned()],
            max_age: Duration::from_secs(7 * 24 * 3600),
            duplicate_window: Duration::from_secs(120),
            ..Default::default()
        })
        .await
        .context("Stream EVENTS konnte nicht angelegt werden")?;
        Ok(Self { client, js })
    }

    /// Veröffentlicht ein Ereignis. Mit `dedup_id` verwirft NATS Duplikate innerhalb
    /// des Fensters, etwa wenn der Broker eine Nachricht erneut zustellt.
    pub async fn publish<T: Serialize>(&self, subject: &str, dedup_id: Option<&str>, event: &T) -> Result<()> {
        let payload = serde_json::to_vec(event)?;
        let mut headers = async_nats::HeaderMap::new();
        if let Some(id) = dedup_id {
            headers.insert("Nats-Msg-Id", id);
        }
        self.js
            .publish_with_headers(subject.to_owned(), headers, payload.into())
            .await
            .with_context(|| format!("Veröffentlichen auf {subject} fehlgeschlagen"))?
            .await
            .with_context(|| format!("Bestätigung für {subject} fehlt"))?;
        Ok(())
    }

    /// Liest Ereignisse mit einem dauerhaften Konsumenten und ruft `handler(subject, nutzlast)` auf.
    ///
    /// Erfolg bestätigt die Nachricht. Ein Fehler stellt sie nach fünf Sekunden
    /// erneut zu, höchstens zehnmal. Die Funktion kehrt nur bei einem Fehler der
    /// Verbindung zurück.
    pub async fn consume<H, Fut>(&self, durable: &str, filters: &[&str], handler: H) -> Result<()>
    where
        H: Fn(String, Vec<u8>) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let stream = self
            .js
            .get_stream(subject::STREAM)
            .await
            .context("Stream EVENTS fehlt")?;
        let consumer = stream
            .get_or_create_consumer(
                durable,
                pull::Config {
                    durable_name: Some(durable.to_owned()),
                    filter_subjects: filters.iter().map(|s| (*s).to_owned()).collect(),
                    ack_policy: jetstream::consumer::AckPolicy::Explicit,
                    max_deliver: 10,
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("Konsument {durable} konnte nicht angelegt werden"))?;

        let mut messages = consumer.messages().await.context("Nachrichtenstrom nicht verfügbar")?;
        while let Some(next) = messages.next().await {
            let msg = next.map_err(|e| anyhow!("Nachricht konnte nicht gelesen werden: {e}"))?;
            let subject = msg.subject.to_string();
            match handler(subject.clone(), msg.payload.to_vec()).await {
                Ok(()) => {
                    if let Err(e) = msg.ack().await {
                        tracing::warn!("Bestätigung für {subject} fehlgeschlagen: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Verarbeitung von {subject} fehlgeschlagen: {e:#}");
                    if let Err(e) = msg.ack_with(AckKind::Nak(Some(Duration::from_secs(5)))).await {
                        tracing::warn!("Wiederholung für {subject} nicht vermerkt: {e}");
                    }
                }
            }
        }
        Err(anyhow!("Nachrichtenstrom von {durable} wurde beendet"))
    }

    /// Beantwortet Befehle auf `subject`. Kehrt nur bei einem Fehler der Verbindung zurück.
    pub async fn serve_commands<Req, H, Fut>(&self, subject: &str, handler: H) -> Result<()>
    where
        Req: DeserializeOwned,
        H: Fn(Req) -> Fut,
        Fut: Future<Output = CommandReply>,
    {
        let mut sub = self
            .client
            .subscribe(subject.to_owned())
            .await
            .context("Abonnieren von Befehlen fehlgeschlagen")?;
        while let Some(msg) = sub.next().await {
            let reply = match serde_json::from_slice::<Req>(&msg.payload) {
                Ok(req) => handler(req).await,
                Err(e) => CommandReply::error(format!("Ungültige Anfrage: {e}")),
            };
            if let Some(to) = msg.reply {
                let bytes = serde_json::to_vec(&reply)?;
                if let Err(e) = self.client.publish(to, bytes.into()).await {
                    tracing::warn!("Antwort auf {subject} konnte nicht gesendet werden: {e}");
                }
            }
        }
        Err(anyhow!("Befehlskanal {subject} wurde beendet"))
    }

    /// Sendet einen Befehl und wartet auf die Antwort.
    pub async fn request<Req: Serialize>(&self, subject: &str, req: &Req, timeout: Duration) -> Result<CommandReply> {
        let payload = serde_json::to_vec(req)?;
        let response = tokio::time::timeout(timeout, self.client.request(subject.to_owned(), payload.into()))
            .await
            .map_err(|_| anyhow!("Keine Antwort auf {subject} innerhalb von {} s", timeout.as_secs()))?
            .map_err(|e| anyhow!("Befehl {subject} nicht zustellbar: {e}"))?;
        serde_json::from_slice(&response.payload).context("Antwort ist kein gültiger Befehlsbescheid")
    }

    /// Abonniert alle Ereignisse ohne Zwischenspeicher, für die Live-Anzeige der API.
    pub async fn subscribe_events(&self) -> Result<async_nats::Subscriber> {
        self.client
            .subscribe(subject::ALL_EVENTS.to_owned())
            .await
            .context("Abonnieren der Ereignisse fehlgeschlagen")
    }

    /// Ob die Verbindung zu NATS steht.
    pub fn is_connected(&self) -> bool {
        self.client.connection_state() == async_nats::connection::State::Connected
    }
}
