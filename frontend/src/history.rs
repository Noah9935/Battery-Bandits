//! Protokoll: wer hat wann was getan. Für alle Geräte (Wartung) oder für ein Gerät (Detailpanel).

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::ActivityEntry;

use shared::TaskComment;

use crate::state::Store;
use crate::{api, jsutil};

/// Ist der Eintrag ein Kommentar, dessen Inhalt sich ausklappen lässt?
pub fn is_comment_entry(entry: &ActivityEntry) -> bool {
    entry.task_id.is_some() && entry.text.contains("kommentiert")
}

/// Ein Eintrag des Protokolls. Bei Kommentaren klappt ein Tipp den Wortlaut aus.
#[component]
fn LogEntry(entry: ActivityEntry) -> impl IntoView {
    let open = RwSignal::new(false);
    let comments = RwSignal::new(None::<Result<Vec<TaskComment>, String>>);
    let task_id = entry.task_id;
    let expandable = is_comment_entry(&entry);
    let toggle = move |_| {
        let now_open = !open.get_untracked();
        open.set(now_open);
        if now_open
            && comments.with_untracked(Option::is_none)
            && let Some(id) = task_id
        {
            spawn_local(async move {
                comments.set(Some(api::comments(id).await));
            });
        }
    };
    view! {
        <li class="log-entry">
            <span class="muted log-time">{jsutil::date_time_label(entry.at)}</span>
            <span class="log-text">{entry.text}</span>
            {expandable
                .then(|| {
                    view! {
                        <button class="link log-toggle" aria-expanded=move || open.get().to_string() on:click=toggle>
                            {move || if open.get() { "Kommentare ausblenden" } else { "Kommentare lesen" }}
                        </button>
                    }
                })}
            <Show when=move || open.get()>
                <ul class="log-comments">
                    {move || match comments.get() {
                        None => view! { <li class="muted">"Wird geladen."</li> }.into_any(),
                        Some(Err(e)) => view! { <li class="form-error">{e}</li> }.into_any(),
                        Some(Ok(list)) if list.is_empty() => view! { <li class="muted">"Keine Kommentare."</li> }.into_any(),
                        Some(Ok(list)) => {
                            list.into_iter()
                                .map(|c| {
                                    view! {
                                        <li>
                                            <span class="comment-head">
                                                <strong>{c.author_name}</strong>
                                                <span class="muted mono-num">{jsutil::date_time_label(c.created_at)}</span>
                                            </span>
                                            <span class="comment-body">{c.body}</span>
                                        </li>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }
                    }}
                </ul>
            </Show>
        </li>
    }
}

/// Ändert sich diese Zahlenfolge, hat jemand etwas getan und das Protokoll wird neu geladen.
fn signature(s: Store) -> (usize, i64, usize, usize) {
    s.tasks.with(|tasks| {
        (
            tasks.len(),
            tasks.iter().map(|t| t.comment_count).sum(),
            tasks
                .iter()
                .filter(|t| t.status == shared::MaintenanceStatus::Planned)
                .count(),
            s.alerts.with(Vec::len),
        )
    })
}

#[component]
pub fn History(#[prop(optional, into)] device: Option<String>, limit: u32) -> impl IntoView {
    let s = expect_context::<Store>();
    let entries = RwSignal::new(Vec::<ActivityEntry>::new());
    let failed = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);
    let shown = RwSignal::new(crate::limit::STEP);
    let total = Signal::derive(move || entries.with(Vec::len));

    Effect::new(move |_| {
        let _changed = signature(s);
        let device = device.clone();
        spawn_local(async move {
            match api::activity(device.as_deref(), limit).await {
                Ok(list) => {
                    entries.set(list);
                    failed.set(None);
                }
                Err(e) => failed.set(Some(e)),
            }
            loaded.set(true);
        });
    });

    view! {
        {move || {
            if let Some(e) = failed.get() {
                return view! { <p class="empty">{e}</p> }.into_any();
            }
            let list = entries.get();
            if list.is_empty() {
                let text = if loaded.get() { "Noch nichts protokolliert." } else { "Protokoll wird geladen." };
                return view! { <p class="muted">{text}</p> }.into_any();
            }
            view! {
                <ul class="log">
                    {list
                        .into_iter()
                        .take(shown.get())
                        .map(|e| view! { <LogEntry entry=e/> })
                        .collect_view()}
                </ul>
                <crate::limit::MoreButton total=total shown=shown/>
            }
                .into_any()
        }}
    }
}
