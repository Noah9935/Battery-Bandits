//! Echtzeit-Push: `LISTEN domain_events` → Broadcast → Server-Sent Events.
//!
//! Jede Nachricht ist ein DomainEvent als JSON (Feld `type`). Verliert ein Client Nachrichten
//! (zu langsam) oder reißt die Datenbankverbindung ab, bekommt er das Ereignis `resync` und
//! lädt dann seine Daten neu. Alle 15 Sekunden geht ein Kommentar als Lebenszeichen hinaus.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::Sse;
use axum::response::sse::{Event, KeepAlive};
use futures_util::Stream;
use shared::events::CHANNEL;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::AppState;

pub type EventBus = broadcast::Sender<String>;

/// Marker auf dem Bus, wenn Ereignisse verloren gegangen sein könnten.
pub const RESYNC: &str = "{\"type\":\"Resync\"}";

pub fn new_bus() -> EventBus {
    broadcast::channel(256).0
}

/// Leitet Ereignisse aus der Datenbank auf den Bus. Läuft bis zum Ende des Programms.
pub fn spawn_pump(pool: PgPool, bus: EventBus) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let mut listener = match PgListener::connect_with(&pool).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("LISTEN nicht möglich: {e}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };
            if let Err(e) = listener.listen(CHANNEL).await {
                tracing::warn!("LISTEN fehlgeschlagen: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
            tracing::info!("Ereignis-Pumpe verbunden");
            loop {
                match listener.recv().await {
                    Ok(n) => {
                        let _ = bus.send(n.payload().to_owned());
                    }
                    Err(e) => {
                        tracing::warn!("Ereignis-Pumpe gestört: {e}");
                        let _ = bus.send(RESYNC.to_owned());
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        break;
                    }
                }
            }
        }
    })
}

/// Wandelt eine Bus-Nachricht in ein SSE-Ereignis.
pub fn to_sse(message: Result<String, BroadcastStreamRecvError>) -> Event {
    match message {
        Ok(json) if json == RESYNC => Event::default().event("resync").data("{}"),
        Ok(json) => Event::default().data(json),
        // Der Empfänger war zu langsam und hat Nachrichten verpasst.
        Err(BroadcastStreamRecvError::Lagged(_)) => Event::default().event("resync").data("{}"),
    }
}

/// `GET /api/events`
pub async fn events(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.bus.subscribe()).map(|m| Ok(to_sse(m)));
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("lebenszeichen"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(event: Event) -> String {
        // Event hat keinen öffentlichen Zugriff; die Debug-Ausgabe enthält Name und Daten.
        format!("{event:?}")
    }

    #[test]
    fn normal_messages_become_default_events_with_data() {
        let text = render(to_sse(Ok("{\"type\":\"ReadingSaved\"}".to_owned())));
        assert!(text.contains("ReadingSaved"));
        assert!(!text.contains("resync"));
    }

    #[test]
    fn resync_marker_and_lag_become_resync_events() {
        assert!(render(to_sse(Ok(RESYNC.to_owned()))).contains("resync"));
        assert!(render(to_sse(Err(BroadcastStreamRecvError::Lagged(5)))).contains("resync"));
    }

    #[tokio::test]
    async fn bus_delivers_to_every_subscriber() {
        let bus = new_bus();
        let (mut a, mut b) = (bus.subscribe(), bus.subscribe());
        bus.send("x".into()).unwrap();
        assert_eq!(a.recv().await.unwrap(), "x");
        assert_eq!(b.recv().await.unwrap(), "x");
    }
}
