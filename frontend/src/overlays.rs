//! Überlagerungen: Befehlspalette (Strg+K) und Hilfe zu den Tastenkürzeln.

use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use shared::DeviceStatus;

use crate::api;
use crate::features::Feature;
use crate::filter::{self, ViewFilter};
use crate::icons::Icon;
use crate::search::{self, Item, SHORTCUTS, Target};
use crate::state::{ModalKind, Page, Store};
use crate::{format, jsutil, share};
use leptos::task::spawn_local;
use shared::{NewMaintenanceTask, TaskKind, TaskPriority};

use crate::tickets::TicketDialog;

/// Nächster Modus beim Durchschalten: automatisch, hell, dunkel.
pub fn next_theme(mode: &str) -> &'static str {
    match mode {
        "auto" => "light",
        "light" => "dark",
        _ => "auto",
    }
}

/// Lädt die aktuell gefilterte Geräteliste als CSV herunter.
pub fn export_filtered(s: Store) {
    let filter = s.filter.get_untracked();
    let pinned = if s.on(Feature::Favorites) {
        s.favorites.get_untracked()
    } else {
        Default::default()
    };
    let rows = s
        .devices
        .with_untracked(|devices| filter::apply_pinned(devices, &filter, &pinned));
    jsutil::download_text(
        "battery-bandits-geraete.csv",
        &share::devices_to_csv(&rows),
        "text/csv;charset=utf-8",
    );
    s.say(format!("{} Geräte als CSV gespeichert.", rows.len()));
}

fn items(s: Store) -> Vec<Item> {
    let mut list: Vec<Item> = Page::ALL
        .into_iter()
        .map(|page| Item {
            label: page.label().to_owned(),
            hint: "Seite".to_owned(),
            target: Target::Page(page),
        })
        .collect();
    list.push(Item {
        label: "Farbschema wechseln".into(),
        hint: "Aktion, Taste T".into(),
        target: Target::ToggleTheme,
    });
    list.push(Item {
        label: "Nur kritische Geräte zeigen".into(),
        hint: "Aktion".into(),
        target: Target::FilterCritical,
    });
    list.push(Item {
        label: "Filter zurücksetzen".into(),
        hint: "Aktion".into(),
        target: Target::ClearFilters,
    });
    if s.on(Feature::ExportAll) {
        list.push(Item {
            label: "Geräteliste als CSV exportieren".into(),
            hint: "Aktion".into(),
            target: Target::ExportCsv,
        });
    }
    list.push(Item {
        label: "Tastenkürzel anzeigen".into(),
        hint: "Hilfe, Taste ?".into(),
        target: Target::Help,
    });
    s.devices.with(|devices| {
        for d in devices {
            let name = d.name.clone().unwrap_or_default();
            list.push(Item {
                label: if name.is_empty() {
                    d.id.clone()
                } else {
                    format!("{} {}", d.id, name)
                },
                hint: format!(
                    "{}, {}",
                    format::source_label(d.data_source),
                    format::status_label(d.status)
                ),
                target: Target::Device(d.id.clone()),
            });
        }
    });
    list
}

fn run(s: Store, target: Target) {
    s.palette_open.set(false);
    match target {
        Target::Page(page) => s.page.set(page),
        Target::Device(id) => s.selected.set(Some(id)),
        Target::ToggleTheme => {
            let mode = next_theme(&s.theme_mode.get_untracked());
            s.set_theme(mode);
        }
        Target::FilterCritical => {
            s.page.set(Page::Overview);
            s.filter.update(|f| f.status = Some(DeviceStatus::Critical));
        }
        Target::ClearFilters => s.filter.set(ViewFilter::default()),
        Target::ExportCsv => export_filtered(s),
        Target::Help => s.help_open.set(true),
    }
}

#[component]
pub fn CommandPalette() -> impl IntoView {
    let s = expect_context::<Store>();
    let query = RwSignal::new(String::new());
    let cursor = RwSignal::new(0usize);
    let results = Memo::new(move |_| search::search(&items(s), &query.get(), 8));

    Effect::new(move |_| {
        if s.palette_open.get() {
            query.set(String::new());
            cursor.set(0);
            Timeout::new(30, || jsutil::focus_element("palette-input")).forget();
        }
    });

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let count = results.with(Vec::len);
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                cursor.update(|c| *c = if count == 0 { 0 } else { (*c + 1) % count });
            }
            "ArrowUp" => {
                ev.prevent_default();
                cursor.update(|c| *c = if count == 0 { 0 } else { (*c + count - 1) % count });
            }
            "Enter" => {
                if let Some(item) = results.with(|r| r.get(cursor.get_untracked()).cloned()) {
                    run(s, item.target);
                }
            }
            _ => {}
        }
    };

    view! {
        {move || {
            s.palette_open
                .get()
                .then(|| {
                    view! {
                        <div class="scrim" on:click=move |_| s.palette_open.set(false)></div>
                        <div class="palette" role="dialog" aria-modal="true" aria-label="Befehlspalette">
                            <label class="palette-input">
                                <Icon name="command"/>
                                <input
                                    id="palette-input"
                                    type="text"
                                    autocomplete="off"
                                    placeholder="Gerät, Seite oder Aktion suchen"
                                    prop:value=move || query.get()
                                    on:input=move |ev| {
                                        query.set(event_target_value(&ev));
                                        cursor.set(0);
                                    }
                                    on:keydown=on_key
                                />
                            </label>
                            <ul class="palette-list">
                                {move || {
                                    let list = results.get();
                                    if list.is_empty() {
                                        return view! { <li class="palette-empty muted">"Nichts gefunden."</li> }.into_any();
                                    }
                                    list.into_iter()
                                        .enumerate()
                                        .map(|(i, item)| {
                                            let target = item.target.clone();
                                            view! {
                                                <li
                                                    class=move || if cursor.get() == i { "palette-item active" } else { "palette-item" }
                                                    on:mouseenter=move |_| cursor.set(i)
                                                    on:click=move |_| run(s, target.clone())
                                                >
                                                    <span>{item.label}</span>
                                                    <span class="muted">{item.hint}</span>
                                                </li>
                                            }
                                        })
                                        .collect_view()
                                        .into_any()
                                }}
                            </ul>
                        </div>
                    }
                })
        }}
    }
}

fn device_name(s: Store, id: &str) -> String {
    format::device_title(id, s.device(id).as_ref().and_then(|d| d.name.as_deref()))
}

/// Kurzer Steckbrief des Geräts für die Dialoge.
fn device_summary(s: Store, id: &str) -> impl IntoView + use<> {
    let device = s.device(id);
    let rows: Vec<(&'static str, String)> = match device {
        Some(d) => vec![
            ("Status", format::status_label(d.status).to_owned()),
            (
                "Ladestand",
                d.soc_percent.map_or_else(|| "unbekannt".to_owned(), format::percent),
            ),
            (
                "Letzte Meldung",
                d.last_seen_at
                    .map_or_else(|| "noch keine".to_owned(), jsutil::date_time_label),
            ),
            (
                "Position",
                match (d.lat, d.lon) {
                    (Some(lat), Some(lon)) => format!("{}, {}", format::coordinate(lat), format::coordinate(lon)),
                    _ => "Kein GPS-Fix".to_owned(),
                },
            ),
        ],
        None => Vec::new(),
    };
    view! {
        <dl class="kv-list">
            {rows
                .into_iter()
                .map(|(label, value)| view! { <div class="kv"><dt>{label}</dt><dd class="tabular">{value}</dd></div> })
                .collect_view()}
        </dl>
    }
}

#[component]
fn PlanForm(device_id: String, alert_id: Option<i64>) -> impl IntoView {
    let s = expect_context::<Store>();
    let note = RwSignal::new(String::new());
    let default_kind = alert_id
        .and_then(|id| {
            s.alerts
                .with_untracked(|a| a.iter().find(|a| a.id == id).map(|a| a.kind))
        })
        .map_or(TaskKind::Battery, |k| match k {
            shared::AlertKind::Critical | shared::AlertKind::Offline => TaskKind::Battery,
            _ => TaskKind::Inspection,
        });
    let kind = RwSignal::new(default_kind.as_str().to_owned());
    let priority = RwSignal::new(TaskPriority::Normal.as_str().to_owned());
    let assignee = RwSignal::new(String::new());
    let due = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let problem = RwSignal::new(None::<String>);
    let title = device_name(s, &device_id);
    let summary = device_summary(s, &device_id);
    let submit = {
        let device_id = device_id.clone();
        move |_| {
            busy.set(true);
            let text = note.get_untracked();
            let new = NewMaintenanceTask {
                device_id: device_id.clone(),
                alert_id,
                note: Some(text.trim().to_owned()).filter(|n| !n.is_empty()),
                kind: kind.get_untracked().parse().unwrap_or_default(),
                priority: priority.get_untracked().parse().unwrap_or_default(),
                assignee: Some(assignee.get_untracked()).filter(|a| !a.is_empty()),
                due_date: chrono::NaiveDate::parse_from_str(&due.get_untracked(), "%Y-%m-%d").ok(),
            };
            spawn_local(async move {
                match api::plan_task(&new).await {
                    Ok(_) => {
                        s.modal.set(None);
                        s.say("Einsatz geplant.");
                        s.refresh_soon();
                    }
                    Err(e) => {
                        problem.set(Some(e));
                        busy.set(false);
                    }
                }
            });
        }
    };
    view! {
        <h2>"Einsatz planen"</h2>
        <p class="muted">{title}</p>
        {summary}
        <div class="modal-grid">
            <label class="field modal-field">
                <span>"Art"</span>
                <select on:change=move |ev| kind.set(event_target_value(&ev))>
                    {TaskKind::ALL
                        .iter()
                        .map(|k| view! { <option value=k.as_str() selected=*k == default_kind>{k.label()}</option> })
                        .collect_view()}
                </select>
            </label>
            <label class="field modal-field">
                <span>"Dringlichkeit"</span>
                <select on:change=move |ev| priority.set(event_target_value(&ev))>
                    {[TaskPriority::Low, TaskPriority::Normal, TaskPriority::High]
                        .iter()
                        .map(|p| {
                            view! { <option value=p.as_str() selected=*p == TaskPriority::Normal>{p.label()}</option> }
                        })
                        .collect_view()}
                </select>
            </label>
            <label class="field modal-field">
                <span>"Zuständig"</span>
                <select on:change=move |ev| assignee.set(event_target_value(&ev))>
                    <option value="" selected=true>"Noch niemand"</option>
                    {move || {
                        let me = s.me.with(|m| m.as_ref().map(|m| m.username.clone()));
                        s.users
                            .get()
                            .into_iter()
                            .map(|u| {
                                let label = if Some(&u.username) == me.as_ref() {
                                    format!("{} (ich)", u.display_name)
                                } else {
                                    u.display_name.clone()
                                };
                                view! { <option value=u.username.clone()>{label}</option> }
                            })
                            .collect_view()
                    }}
                </select>
            </label>
            <label class="field modal-field">
                <span>"Frist (optional)"</span>
                <input
                    type="date"
                    prop:value=move || due.get()
                    on:input=move |ev| due.set(event_target_value(&ev))
                />
            </label>
        </div>
        <label class="field modal-field">
            <span>"Hinweis für die Person vor Ort (optional)"</span>
            <textarea
                rows="3"
                placeholder="Zum Beispiel: Schaltkasten an der Rückseite, Ersatzbatterie mitnehmen"
                prop:value=move || note.get()
                on:input=move |ev| note.set(event_target_value(&ev))
            ></textarea>
        </label>
        {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
        <div class="modal-actions">
            <button class="btn" on:click=move |_| s.modal.set(None)>"Abbrechen"</button>
            <button class="btn primary" disabled=move || busy.get() on:click=submit>"Einsatz planen"</button>
        </div>
    }
}

#[component]
fn ConfirmBox(
    title: &'static str,
    text: String,
    device_id: String,
    confirm_label: &'static str,
    danger: bool,
    #[prop(into)] on_confirm: Callback<(), ()>,
) -> impl IntoView {
    let s = expect_context::<Store>();
    let device = device_name(s, &device_id);
    view! {
        <h2>{title}</h2>
        <p class="muted">{device}</p>
        <p class="modal-text">{text}</p>
        <div class="modal-actions">
            <button class="btn" on:click=move |_| s.modal.set(None)>"Abbrechen"</button>
            <button class=if danger { "btn btn-danger" } else { "btn primary" } on:click=move |_| on_confirm.run(())>
                {confirm_label}
            </button>
        </div>
    }
}

#[component]
pub fn Modals() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            s.modal
                .get()
                .map(|kind| {
                    let body = match kind {
                        ModalKind::Plan { device_id, alert_id } => view! { <PlanForm device_id=device_id alert_id=alert_id/> }.into_any(),
                        ModalKind::Dismiss { alert_id, device_id } => {
                            view! {
                                <ConfirmBox
                                    title="Meldung abweisen?"
                                    text="Die Meldung verschwindet aus der Liste. Das Gerät selbst bleibt unverändert: Ist es weiterhin kritisch oder offline, kann später eine neue Meldung entstehen."
                                        .to_owned()
                                    device_id=device_id
                                    confirm_label="Ja, abweisen"
                                    danger=true
                                    on_confirm=move |()| {
                                        spawn_local(async move {
                                            match api::dismiss_alert(alert_id).await {
                                                Ok(_) => s.say("Meldung abgewiesen."),
                                                Err(e) => s.say(e),
                                            }
                                            s.modal.set(None);
                                            s.refresh_soon();
                                        });
                                    }
                                />
                            }
                                .into_any()
                        }
                        ModalKind::Ticket { task_id } => view! { <TicketDialog task_id=task_id/> }.into_any(),
                        ModalKind::Complete { task_id, device_id } => {
                            let battery = s
                                .tasks
                                .with_untracked(|t| t.iter().find(|t| t.id == task_id).map(|t| t.kind))
                                .is_none_or(|k| k == TaskKind::Battery);
                            view! {
                                <ConfirmBox
                                    title=if battery { "Batterie wirklich gewechselt?" } else { "Einsatz wirklich erledigt?" }
                                    text=if battery {
                                        "Der Einsatz wird als erledigt gemeldet und die offene Meldung geschlossen. Der Ladezustand wird mit der nächsten Messung neu bestimmt."
                                    } else {
                                        "Der Einsatz wird als erledigt gemeldet. An der Meldung ändert sich nichts: Sie bleibt offen, bis die Messwerte wieder in Ordnung sind."
                                    }
                                        .to_owned()
                                    device_id=device_id
                                    confirm_label=if battery { "Ja, gewechselt" } else { "Ja, erledigt" }
                                    danger=false
                                    on_confirm=move |()| {
                                        spawn_local(async move {
                                            match api::complete_task(task_id).await {
                                                Ok(_) => s.say(if battery { "Batteriewechsel bestätigt." } else { "Einsatz erledigt." }),
                                                Err(e) => s.say(e),
                                            }
                                            s.modal.set(None);
                                            s.refresh_soon();
                                        });
                                    }
                                />
                            }
                                .into_any()
                        }
                    };
                    view! {
                        <div class="scrim modal-scrim" on:click=move |_| s.modal.set(None)></div>
                        <div class="modal" role="dialog" aria-modal="true">{body}</div>
                    }
                })
        }}
    }
}

#[component]
pub fn HelpDialog() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            s.help_open
                .get()
                .then(|| {
                    view! {
                        <div class="scrim" on:click=move |_| s.help_open.set(false)></div>
                        <div class="palette help" role="dialog" aria-modal="true" aria-label="Tastenkürzel">
                            <h2>"Tastenkürzel"</h2>
                            <dl class="kv-list">
                                {SHORTCUTS
                                    .iter()
                                    .map(|(key, text)| {
                                        view! {
                                            <div class="kv">
                                                <dt>{*text}</dt>
                                                <dd><kbd>{*key}</kbd></dd>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </dl>
                        </div>
                    }
                })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycles_through_all_three_modes() {
        assert_eq!(next_theme("auto"), "light");
        assert_eq!(next_theme("light"), "dark");
        assert_eq!(next_theme("dark"), "auto");
        assert_eq!(next_theme("quatsch"), "auto");
    }
}
