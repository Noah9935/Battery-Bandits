//! Überlagerungen: Befehlspalette (Strg+K) und Hilfe zu den Tastenkürzeln.

use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use shared::DeviceStatus;

use crate::features::Feature;
use crate::filter::{self, ViewFilter};
use crate::icons::Icon;
use crate::search::{self, Item, SHORTCUTS, Target};
use crate::state::{Page, Store};
use crate::{format, jsutil, share};

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
