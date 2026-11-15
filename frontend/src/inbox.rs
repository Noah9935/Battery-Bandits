//! Postfach: was dir zugewiesen wurde, wer dich angeschrieben hat und welche neuen Meldungen es gibt.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::InboxItem;

use crate::api;
use crate::device::DeviceLink;
use crate::forward::{ForwardCard, ForwardMenu};
use crate::icons::Icon;
use crate::jsutil;
use crate::nodes::NodeSubscriptions;
use crate::pager::Pager;
use crate::state::{ModalKind, Store};

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "alert" => "Meldung",
        "assigned" => "Zuweisung",
        "comment" => "Kommentar",
        "shared" => "Geteilt",
        _ => "Hinweis",
    }
}

/// Einträge je Seite im Postfach.
pub const INBOX_PAGE_SIZE: usize = 10;

/// Was das Postfach zeigt: Meldungen nur von den Nodes, bei denen man eingetragen ist (oder allen, wenn der Schalter
/// auf "Alle" steht). Zuweisungen und Kommentare betreffen einen immer und bleiben stehen. Die Suche prüft Text,
/// Node-Kennung und Art ohne Rücksicht auf Groß- und Kleinschreibung. Die Reihenfolge bleibt wie geliefert.
pub fn visible_items(
    items: &[InboxItem],
    subscribed: &BTreeSet<String>,
    mine_only: bool,
    query: &str,
) -> Vec<InboxItem> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .filter(|i| !mine_only || i.kind != "alert" || i.device_id.as_ref().is_none_or(|d| subscribed.contains(d)))
        .filter(|i| {
            needle.is_empty()
                || i.text.to_lowercase().contains(&needle)
                || i.device_id
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
                || kind_label(&i.kind).to_lowercase().contains(&needle)
        })
        .cloned()
        .collect()
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
    } else if item.kind == "shared" {
        // Eine geteilte Tour: Die Wartung zeigt sie unter Tour, Tour planen.
        s.go_page(crate::state::Page::Maintenance);
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
    let forward_text = item.text.clone();
    let forward_device = item.device_id.clone();
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
            <ForwardMenu text=forward_text device_id=forward_device/>
        </li>
    }
}

#[component]
pub fn Inbox() -> impl IntoView {
    let s = expect_context::<Store>();
    let unread = move || s.inbox.with(|i| unread_count(i));
    let forward_open = RwSignal::new(false);
    let nodes_open = RwSignal::new(false);
    // Standard: nur die Nodes, bei denen man eingetragen ist. Immer nur eine Seite, Lesen ändert die Reihenfolge nicht.
    let mine_only = RwSignal::new(true);
    let query = RwSignal::new(String::new());
    let page = RwSignal::new(0usize);
    let filtered = Memo::new(move |_| {
        let (mine, q) = (mine_only.get(), query.get());
        s.inbox
            .with(|items| s.subscriptions.with(|subs| visible_items(items, subs, mine, &q)))
    });
    let total = Signal::derive(move || filtered.with(Vec::len));
    let all = Signal::derive(move || s.inbox.with(Vec::len));
    Effect::new(move |_| {
        mine_only.track();
        query.track();
        page.set(0);
    });
    let visible = move || {
        let (start, end, _) = crate::filter::page_bounds(total.get(), page.get(), INBOX_PAGE_SIZE);
        filtered.with(|rows| rows[start..end].to_vec())
    };
    view! {
        <div class="view">
            <header class="view-head">
                <h1>"Postfach"</h1>
                <p class="sub">"Eilmeldungen und alles, was dich betrifft: Zuweisungen und Kommentare."</p>
            </header>
            <div class="btn-row inbox-actions">
                <button
                    class=move || if forward_open.get() { "btn is-open" } else { "btn" }
                    aria-expanded=move || forward_open.get().to_string()
                    on:click=move |_| forward_open.update(|o| *o = !*o)
                >
                    <Icon name="share"/>
                    "Weiterleiten"
                </button>
                <button
                    class=move || if nodes_open.get() { "btn is-open" } else { "btn" }
                    aria-expanded=move || nodes_open.get().to_string()
                    on:click=move |_| nodes_open.update(|o| *o = !*o)
                >
                    <Icon name="bell"/>
                    {move || format!("Meine Nodes ({})", s.subscriptions.with(|set| set.len()))}
                </button>
            </div>
            // Beide öffnen ein Fenster über der Seite, statt die Seite auseinanderzuschieben.
            <Show when=move || forward_open.get()>
                <div class="scrim modal-scrim" on:click=move |_| forward_open.set(false)></div>
                <div class="modal modal-wide" role="dialog" aria-modal="true" aria-label="Weiterleitung aufs Handy">
                    <button class="icon-btn modal-x" aria-label="Schließen" on:click=move |_| forward_open.set(false)>
                        <Icon name="x"/>
                    </button>
                    <ForwardCard/>
                </div>
            </Show>
            <Show when=move || nodes_open.get()>
                <div class="scrim modal-scrim" on:click=move |_| nodes_open.set(false)></div>
                <div class="modal modal-wide" role="dialog" aria-modal="true" aria-label="Meine Nodes">
                    <button class="icon-btn modal-x" aria-label="Schließen" on:click=move |_| nodes_open.set(false)>
                        <Icon name="x"/>
                    </button>
                    <NodeSubscriptions/>
                </div>
            </Show>
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
                <div id="inbox-toolbar" class="inbox-toolbar" role="search" aria-label="Postfach filtern">
                    <div class="tabs" role="group" aria-label="Anzeigen">
                        <button
                            class=move || if mine_only.get() { "seg-btn is-active" } else { "seg-btn" }
                            aria-pressed=move || mine_only.get().to_string()
                            on:click=move |_| mine_only.set(true)
                        >
                            "Meine Nodes"
                        </button>
                        <button
                            class=move || if mine_only.get() { "seg-btn" } else { "seg-btn is-active" }
                            aria-pressed=move || (!mine_only.get()).to_string()
                            on:click=move |_| mine_only.set(false)
                        >
                            "Alle Meldungen"
                        </button>
                    </div>
                    <input
                        type="search"
                        class="eff-search"
                        placeholder="Suchen, zum Beispiel SIM03"
                        aria-label="Postfach durchsuchen"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                    <span class="muted mono-num eff-count">
                        {move || format!("{} von {}", total.get(), all.get())}
                    </span>
                </div>
                <Pager
                    total=total
                    page=page
                    size=INBOX_PAGE_SIZE
                    label="Seiten des Postfachs (oben)"
                    anchor="inbox-toolbar"
                    top=true
                />
                {move || {
                    if all.get() == 0 {
                        view! { <p class="empty">"Nichts Neues. Sobald dir jemand einen Einsatz zuweist oder eine Meldung eintrifft, steht es hier."</p> }
                            .into_any()
                    } else if total.get() == 0 {
                        view! { <p class="empty">"Hier passt nichts. Unter \"Alle Meldungen\" oder mit einer anderen Suche gibt es mehr."</p> }
                            .into_any()
                    } else {
                        view! {
                            <ul class="inbox-list">
                                <For
                                    each=visible
                                    key=|i| (i.id, i.read)
                                    children=move |i| view! { <Entry item=i/> }
                                />
                            </ul>
                        }
                            .into_any()
                    }
                }}
                <Pager
                    total=total
                    page=page
                    size=INBOX_PAGE_SIZE
                    label="Seiten des Postfachs"
                    anchor="inbox-toolbar"
                />
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
        assert_eq!(kind_label("shared"), "Geteilt");
        assert_eq!(kind_label("sonstwas"), "Hinweis");
    }

    #[test]
    fn mine_shows_alerts_of_subscribed_nodes_and_everything_that_concerns_me() {
        let mk = |id: i64, kind: &str, device: Option<&str>, text: &str| InboxItem {
            id,
            kind: kind.into(),
            device_id: device.map(str::to_owned),
            text: text.into(),
            ..item(false)
        };
        let items = [
            mk(1, "alert", Some("SIM01"), "Batterie schwach"),
            mk(2, "alert", Some("SIM02"), "Node offline"),
            mk(3, "assigned", Some("SIM02"), "Einsatz zugewiesen"),
            mk(4, "alert", None, "Allgemein"),
        ];
        let subs: BTreeSet<String> = ["SIM01".to_owned()].into();
        let ids = |mine, q: &str| -> Vec<i64> { visible_items(&items, &subs, mine, q).iter().map(|i| i.id).collect() };
        assert_eq!(ids(true, ""), [1, 3, 4]);
        assert_eq!(ids(false, ""), [1, 2, 3, 4]);
        assert_eq!(ids(false, "sim02"), [2, 3]);
        assert_eq!(ids(true, "SIM02"), [3]);
        assert_eq!(ids(false, "zuweisung"), [3]);
        assert_eq!(ids(true, "gibtesnicht"), Vec::<i64>::new());
    }

    #[test]
    fn only_unread_entries_count() {
        assert_eq!(unread_count(&[item(false), item(true), item(false)]), 2);
        assert_eq!(unread_count(&[]), 0);
    }
}
