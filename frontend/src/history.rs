//! Protokoll: wer hat wann was getan. Für alle Geräte (Wartung) oder für ein Gerät (Detailpanel).

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::ActivityEntry;

use crate::state::Store;
use crate::{api, jsutil};

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
                        .map(|e| {
                            view! {
                                <li>
                                    <span class="muted log-time">{jsutil::date_time_label(e.at)}</span>
                                    <span>{e.text}</span>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            }
                .into_any()
        }}
    }
}
