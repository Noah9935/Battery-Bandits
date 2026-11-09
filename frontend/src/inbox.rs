//! Postfach: was dir zugewiesen wurde, wer dich angeschrieben hat und welche neuen Meldungen es gibt.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::InboxItem;

use crate::api;
use crate::device::DeviceLink;
use crate::jsutil;
use crate::state::{ModalKind, Store};

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "alert" => "Meldung",
        "assigned" => "Zuweisung",
        "comment" => "Kommentar",
        _ => "Hinweis",
    }
}

pub fn unread_count(items: &[InboxItem]) -> usize {
    items.iter().filter(|i| !i.read).count()
}

/// Öffnet, worauf der Eintrag zeigt: den Einsatz oder das Gerät.
fn open_target(s: Store, item: &InboxItem) {
    if let Some(task_id) = item.task_id {
        s.modal.set(Some(ModalKind::Ticket { task_id }));
    } else if let Some(device) = &item.device_id {
        s.selected.set(Some(device.clone()));
    }
}

fn mark(s: Store, ids: Option<Vec<i64>>) {
    spawn_local(async move {
        if let Err(e) = api::mark_read(ids).await {
            s.say(e);
        }
        s.refresh();
    });
}

#[component]
fn Entry(item: InboxItem) -> impl IntoView {
    let s = expect_context::<Store>();
    let id = item.id;
    let unread = !item.read;
    let target = item.clone();
    let device = item.device_id.clone();
    view! {
        <li class=if unread { "inbox-row unread" } else { "inbox-row" }>
            <div class="inbox-main">
                <span class="inbox-dot" aria-label=if unread { "ungelesen" } else { "gelesen" }></span>
                <span class="origin">{kind_label(&item.kind)}</span>
                <button
                    class="inbox-text"
                    on:click=move |_| {
                        if unread {
                            mark(s, Some(vec![id]));
                        }
                        open_target(s, &target);
                    }
                >
                    {item.text.clone()}
                </button>
                {device.map(|d| view! { <DeviceLink id=d.clone() class="mono-num">{d}</DeviceLink> })}
                <span class="muted inbox-time mono-num">{jsutil::date_time_label(item.at)}</span>
            </div>
        </li>
    }
}

#[component]
pub fn Inbox() -> impl IntoView {
    let s = expect_context::<Store>();
    let unread = move || s.inbox.with(|i| unread_count(i));
    view! {
        <div class="view">
            <header class="view-head">
                <h1>"Postfach"</h1>
                <p class="sub">"Neue Meldungen, Zuweisungen und Kommentare, die dich betreffen."</p>
            </header>
            <section class="surface">
                <div class="surface-head">
                    <h2>
                        {move || match unread() {
                            0 => "Alles gelesen".to_owned(),
                            1 => "1 ungelesen".to_owned(),
                            n => format!("{n} ungelesen"),
                        }}
                    </h2>
                    <button class="btn btn-sm" disabled=move || unread() == 0 on:click=move |_| mark(s, None)>
                        "Alle als gelesen markieren"
                    </button>
                </div>
                {move || {
                    let list = s.inbox.get();
                    if list.is_empty() {
                        view! { <p class="empty">"Nichts Neues. Sobald dir jemand einen Einsatz zuweist oder eine Meldung eintrifft, steht es hier."</p> }
                            .into_any()
                    } else {
                        view! {
                            <ul class="inbox-list">
                                <For each=move || s.inbox.get() key=|i| (i.id, i.read) children=move |i| view! { <Entry item=i/> }/>
                            </ul>
                        }
                            .into_any()
                    }
                }}
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn item(read: bool) -> InboxItem {
        InboxItem {
            id: 1,
            at: DateTime::from_timestamp(0, 0).unwrap(),
            kind: "alert".into(),
            text: "x".into(),
            device_id: None,
            task_id: None,
            read,
        }
    }

    #[test]
    fn kinds_have_german_labels() {
        assert_eq!(kind_label("alert"), "Meldung");
        assert_eq!(kind_label("assigned"), "Zuweisung");
        assert_eq!(kind_label("comment"), "Kommentar");
        assert_eq!(kind_label("sonstwas"), "Hinweis");
    }

    #[test]
    fn only_unread_entries_count() {
        assert_eq!(unread_count(&[item(false), item(true), item(false)]), 2);
        assert_eq!(unread_count(&[]), 0);
    }
}
