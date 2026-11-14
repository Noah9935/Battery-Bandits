//! Standortkarte. Auf dem Handy bleibt die Karte fast ganz frei: oben eine schmale Leiste mit Suche und Filterknopf,
//! unten ein Blatt, das sich bei Bedarf aufklappt. Nichts davon verwischt oder verdeckt die Karte.

use leptos::html;
use leptos::prelude::*;
use shared::{DataSource, DeviceStatus};

use crate::device::DeviceLink;
use crate::features::Feature;
use crate::icons::Icon;
use crate::markers::{MapFilter, markers_json, missing_fix_text, visible};
use crate::pins::{PinButton, route_memos};
use crate::state::{Page, Store};
use crate::{format, jsutil};

fn toggle<T: Ord + Copy>(
    filter: RwSignal<MapFilter>,
    item: T,
    pick: impl Fn(&mut MapFilter) -> &mut std::collections::BTreeSet<T>,
) {
    filter.update(|f| {
        let set = pick(f);
        if !set.remove(&item) {
            set.insert(item);
        }
    });
}

#[component]
pub fn MapView() -> impl IntoView {
    let s = expect_context::<Store>();
    let node = NodeRef::<html::Div>::new();
    let ready = RwSignal::new(false);
    let fit_tick = RwSignal::new(0u32);
    let focused = RwSignal::new(None::<String>);
    let filter = RwSignal::new(MapFilter::default());
    let last_fit = StoredValue::new(0u32);
    let first = StoredValue::new(true);
    let filters_open = RwSignal::new(false);
    // Auswahl-Modus: Tipps auf Marker wählen die Nodes aus, die man anfahren und wechseln will (die Merkliste).
    let selecting = RwSignal::new(false);
    let selected_count = Memo::new(move |_| s.favorites.with(std::collections::BTreeSet::len));
    let sheet_open = RwSignal::new(!jsutil::is_narrow());
    // Straßenkarte, einfache Grautöne oder Luftbild. Die Wahl bleibt in diesem Browser gespeichert.
    let base = RwSignal::new(match jsutil::storage_get("bb-map-base").as_deref() {
        Some("sat") => "sat",
        Some("simple") => "simple",
        _ => "map",
    });
    // Treffer der Suche unter dem Suchfeld, ein Tipp springt zum Node.
    let suggest = RwSignal::new(false);
    // Vor dem Öffnen der Details merkt sich die Karte, ob die Geräteliste offen war, und stellt das beim Zurück wieder her.
    let sheet_before = StoredValue::new(None::<bool>);

    // Die Karte wird im nächsten Bild aufgebaut: Der Tipp auf den Karten-Reiter reagiert sofort, die Karte
    // erscheint einen Wimpernschlag danach.
    node.on_load(move |_| {
        jsutil::next_frame(move || {
            let ok = jsutil::map_init("bb-map", move |id| {
                focused.set(Some(id.clone()));
                if selecting.get_untracked() {
                    // Auswahl-Modus: Ein Tipp wählt den Node aus oder ab, statt seine Details zu öffnen.
                    s.toggle_pin(&id);
                } else {
                    s.selected.set(Some(id));
                }
            });
            ready.set(ok);
        });
    });
    on_cleanup(jsutil::map_destroy);

    // Die Seite bleibt im Speicher, auch wenn sie nicht zu sehen ist. Beim Zurückkehren misst die Karte sich neu.
    Effect::new(move |_| {
        if s.page.get() == Page::Map && s.device_page.with(Option::is_none) {
            jsutil::next_frame(jsutil::map_invalidate);
        }
    });

    // Kartengrundlage setzen, sobald die Karte steht und wenn sich Wahl oder Farbschema ändern.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let kind = base.get();
        s.theme_mode.track();
        jsutil::storage_set("bb-map-base", kind);
        jsutil::next_frame(move || jsutil::map_set_base(kind));
    });

    // Marker bei jeder Änderung von Geräten oder Filter neu setzen. Der Ausschnitt passt sich beim ersten Mal,
    // nach einem Klick auf "Alle zeigen" und nach jeder Filteränderung an, damit man das Gefilterte sieht.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let f = filter.get();
        let tick = fit_tick.get();
        // Die Auswahl zählt nur, wenn "nur ausgewählte" an ist: Sonst setzt ein Tipp im Auswahl-Modus die Marker nicht neu.
        let chosen = if f.only_selected {
            s.favorites.get()
        } else {
            Default::default()
        };
        let json = s.devices.with(|devices| markers_json(&visible(devices, &f, &chosen)));
        let fit = first.get_value() && json != "[]" || tick != last_fit.get_value();
        if json != "[]" {
            first.set_value(false);
        }
        last_fit.set_value(tick);
        jsutil::map_set_markers(&json, fit);
    });
    // Der eigene Standort bekommt eine eigene Markierung. Beim ersten Mal und auf Knopfdruck springt die Karte dorthin.
    let jumped = StoredValue::new(false);
    let want_center = StoredValue::new(false);
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        if let Some((lat, lon)) = s.position.get() {
            let center = !jumped.get_value() || want_center.get_value();
            jumped.set_value(true);
            want_center.set_value(false);
            jsutil::map_set_me(lat, lon, center);
        }
    });
    // Ist der Standort noch nicht bekannt, fragt die Karte einmal danach (der Browser bittet dann um Erlaubnis).
    if s.position.with_untracked(Option::is_none) {
        s.locate();
    }
    let to_me = move |_| {
        want_center.set_value(true);
        match s.position.get_untracked() {
            Some((lat, lon)) => {
                want_center.set_value(false);
                jsutil::map_set_me(lat, lon, true);
                s.locate();
            }
            None => {
                // Ohne Freigabe für den Standort nimmt die Karte den eingetragenen Standardstandort.
                if let Some((lat, lon)) = s.home.get_untracked() {
                    jsutil::map_set_me(lat, lon, true);
                    s.say("Der Browser gibt den Standort nicht frei: Die Karte zeigt deinen Standardstandort.");
                }
                s.locate();
            }
        }
    };

    // Angepinnte Geräte hebt die Karte hervor.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let ids: Vec<String> = s.favorites.with(|f| f.iter().cloned().collect());
        // Nach den Markern setzen: Die Reihenfolge der Effekte ist die Reihenfolge im Quelltext.
        jsutil::map_set_pins(&ids);
        s.devices.track();
    });

    // "Auf der Karte zeigen" aus dem Gerät: sobald Karte und Marker stehen, zoomt sie auf das Gerät.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let Some(id) = s.map_goto.get() else { return };
        s.map_goto.set(None);
        jsutil::next_frame(move || {
            jsutil::map_invalidate();
            if jsutil::map_focus(&id) {
                focused.set(Some(id));
            } else {
                s.say("Dieses Gerät hat keine Position auf der Karte.");
            }
        });
    });

    // Wird ein Gerät geöffnet (Marker, Details, Seite), räumen Blatt und Filter die Karte frei.
    // Schließt man die Details wieder, ist die Geräteliste so offen oder zu wie vorher.
    Effect::new(move |_| {
        if s.selected.with(Option::is_some) {
            if sheet_before.get_value().is_none() {
                sheet_before.set_value(Some(sheet_open.get_untracked()));
            }
            sheet_open.set(false);
            filters_open.set(false);
        } else if let Some(was_open) = sheet_before.get_value() {
            sheet_before.set_value(None);
            sheet_open.set(was_open);
        }
    });

    let shown = Memo::new(move |_| {
        let f = filter.get();
        let chosen = if f.only_selected {
            s.favorites.get()
        } else {
            Default::default()
        };
        s.devices
            .with(|devices| visible(devices, &f, &chosen).into_iter().cloned().collect::<Vec<_>>())
    });
    // Nur die ausgewählten Nodes zeigen oder wieder alle. Der Ausschnitt passt sich an die Marker an.
    let toggle_only_selected = move |_| {
        filter.update(|f| f.only_selected = !f.only_selected);
        fit_tick.update(|t| *t += 1);
    };
    // Springt zu einem Node aus der Suche: Der Filter geht weg, damit wieder alle Marker da sind.
    let jump_to = move |id: String, has_position: bool| {
        suggest.set(false);
        if !has_position {
            s.say("Dieses Gerät hat keine Position auf der Karte.");
            return;
        }
        filter.update(|f| f.search.clear());
        focused.set(Some(id.clone()));
        // Erst nach dem Neusetzen der Marker springen, sonst gibt es den Marker noch nicht.
        jsutil::next_frame(move || {
            jsutil::next_frame(move || {
                jsutil::map_focus(&id);
            });
        });
        if jsutil::is_narrow() {
            sheet_open.set(false);
        }
    };
    let suggestions = Memo::new(move |_| {
        if filter.with(|f| f.search.trim().is_empty()) {
            return Vec::new();
        }
        shown.with(|list| list.iter().take(6).cloned().collect::<Vec<_>>())
    });
    let without_fix = move || shown.with(|list| list.iter().filter(|d| d.lat.is_none() || d.lon.is_none()).count());
    let (_, route_url) = route_memos(s);
    let active_filters = move || filter.with(MapFilter::is_active);
    view! {
        <div class="view view-map">
            <h1 class="sr-only">"Standortkarte"</h1>
            <div id="bb-map" class="map" node_ref=node role="region" aria-label="Karte der TrafficNodes"></div>
            <div class="map-top" role="search" aria-label="Kartenfilter">
                <label class="map-search">
                    <Icon name="search"/>
                    <input
                        type="search"
                        placeholder="Gerät suchen"
                        aria-label="Gerät auf der Karte suchen"
                        autocomplete="off"
                        prop:value=move || filter.with(|f| f.search.clone())
                        on:focus=move |_| suggest.set(true)
                        on:blur=move |_| suggest.set(false)
                        on:input=move |ev| {
                            filter.update(|f| f.search = event_target_value(&ev));
                            suggest.set(true);
                        }
                        on:keydown=move |ev| {
                            match ev.key().as_str() {
                                "Enter" => {
                                    if let Some(d) = suggestions.with(|l| l.first().cloned()) {
                                        jump_to(d.id.clone(), d.lat.is_some() && d.lon.is_some());
                                    }
                                }
                                "Escape" => suggest.set(false),
                                _ => {}
                            }
                        }
                    />
                    <Show when=move || suggest.get() && filter.with(|f| !f.search.trim().is_empty())>
                        // Mausdruck nicht ans Feld weitergeben: Das Feld behält den Fokus, bis der Tipp angekommen ist.
                        <ul class="map-suggest surface" role="listbox" on:mousedown=move |ev| ev.prevent_default()>
                            {move || {
                                let list = suggestions.get();
                                if list.is_empty() {
                                    return view! { <li class="empty">"Kein Node passt zur Suche."</li> }.into_any();
                                }
                                list.into_iter()
                                    .map(|d| {
                                        let id = d.id.clone();
                                        let has_position = d.lat.is_some() && d.lon.is_some();
                                        let name = d.name.clone().filter(|n| !n.is_empty());
                                        view! {
                                            <li>
                                                <button
                                                    class="map-suggest-item"
                                                    role="option"
                                                    on:click=move |_| jump_to(id.clone(), has_position)
                                                >
                                                    <span class=format!("dot {}", format::status_class(d.status))></span>
                                                    <span class="map-item-id mono-num">{d.id.clone()}</span>
                                                    <span class="muted map-suggest-name">
                                                        {name.unwrap_or_default()}
                                                    </span>
                                                    {(!has_position).then(|| view! { <span class="muted">"keine Position"</span> })}
                                                </button>
                                            </li>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }}
                        </ul>
                    </Show>
                </label>
                <button
                    class="map-base-btn map-locate"
                    aria-label="Karte auf meinen Standort zentrieren"
                    title=move || {
                        s.position_note
                            .get()
                            .unwrap_or_else(|| "Karte auf deinen aktuellen Standort zentrieren".to_owned())
                    }
                    on:click=to_me
                >
                    <Icon name="locate"/>
                    "Mein Standort"
                </button>
                <div class="map-base-seg" role="group" aria-label="Kartenstil">
                    {[("map", "Karte"), ("simple", "Schwarz-Weiß"), ("sat", "Satellit")]
                        .into_iter()
                        .map(|(kind, label)| {
                            view! {
                                <button
                                    class=move || if base.get() == kind { "is-on" } else { "" }
                                    aria-pressed=move || (base.get() == kind).to_string()
                                    on:click=move |_| base.set(kind)
                                >
                                    {label}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                {move || {
                    s.on(Feature::Favorites)
                        .then(|| {
                            view! {
                                <button
                                    class=move || if selecting.get() { "map-icon-btn is-on" } else { "map-icon-btn" }
                                    aria-label="Nodes zum Wechseln auswählen"
                                    aria-pressed=move || selecting.get().to_string()
                                    title="Nodes auf der Karte auswählen, die man wechseln will"
                                    on:click=move |_| {
                                        selecting.update(|v| *v = !*v);
                                        if selecting.get_untracked() {
                                            filters_open.set(false);
                                            s.selected.set(None);
                                            if jsutil::is_narrow() {
                                                sheet_open.set(false);
                                            }
                                        }
                                    }
                                >
                                    <Icon name="bookmark"/>
                                    <span class="map-count mono-num">{move || selected_count.get()}</span>
                                </button>
                            }
                        })
                }}
                <button
                    class=move || if active_filters() { "map-icon-btn is-on" } else { "map-icon-btn" }
                    aria-label="Filter"
                    aria-expanded=move || filters_open.get().to_string()
                    on:click=move |_| {
                        filters_open.update(|o| *o = !*o);
                        if filters_open.get_untracked() {
                            selecting.set(false);
                        }
                    }
                >
                    <Icon name="filter"/>
                </button>
            </div>
            <Show when=move || selecting.get()>
                <div class="map-panel map-select surface" role="status">
                    <p>
                        <strong class="mono-num">{move || selected_count.get()}</strong>
                        {move || if selected_count.get() == 1 { " Node ausgewählt" } else { " Nodes ausgewählt" }}
                    </p>
                    <p class="muted">
                        "Tippe auf die Marker der Nodes, bei denen du die Batterie wechseln willst. Sie leuchten auf und stehen in der Merkliste unter Wartung."
                    </p>
                    <div class="btn-row">
                        {move || {
                            route_url
                                .get()
                                .map(|href| {
                                    view! {
                                        <a class="btn btn-sm btn-primary" href=href target="_blank" rel="noopener">
                                            <Icon name="navigation"/>
                                            "Route dorthin"
                                        </a>
                                    }
                                })
                        }}
                        <button
                            class=move || if filter.with(|f| f.only_selected) { "btn btn-sm btn-primary" } else { "btn btn-sm" }
                            aria-pressed=move || filter.with(|f| f.only_selected).to_string()
                            on:click=toggle_only_selected
                        >
                            {move || if filter.with(|f| f.only_selected) { "Alle zeigen" } else { "Nur diese zeigen" }}
                        </button>
                        <button
                            class="btn btn-sm"
                            disabled=move || selected_count.get() == 0
                            on:click=move |_| s.clear_favorites()
                        >
                            "Auswahl leeren"
                        </button>
                        <button class="btn btn-sm" on:click=move |_| selecting.set(false)>"Fertig"</button>
                    </div>
                </div>
            </Show>
            <Show when=move || filters_open.get()>
                <div class="map-panel surface">
                    <div class="chips" role="group" aria-label="Status">
                        {DeviceStatus::ALL
                            .iter()
                            .map(|st| {
                                let st = *st;
                                view! {
                                    <button
                                        class=move || if filter.with(|f| f.statuses.contains(&st)) { "chip is-active" } else { "chip" }
                                        aria-pressed=move || filter.with(|f| f.statuses.contains(&st)).to_string()
                                        on:click=move |_| toggle(filter, st, |f| &mut f.statuses)
                                    >
                                        <span class=format!("dot {}", format::status_class(st))></span>
                                        {format::status_label(st)}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                    <div class="chips" role="group" aria-label="Herkunft">
                        {DataSource::ALL
                            .iter()
                            .map(|src| {
                                let src = *src;
                                view! {
                                    <button
                                        class=move || if filter.with(|f| f.sources.contains(&src)) { "chip is-active" } else { "chip" }
                                        aria-pressed=move || filter.with(|f| f.sources.contains(&src)).to_string()
                                        on:click=move |_| toggle(filter, src, |f| &mut f.sources)
                                    >
                                        {format::source_label(src)}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                    {move || {
                        s.on(Feature::Favorites)
                            .then(|| {
                                view! {
                                    <div class="chips" role="group" aria-label="Auswahl">
                                        <button
                                            class=move || if filter.with(|f| f.only_selected) { "chip is-active" } else { "chip" }
                                            aria-pressed=move || filter.with(|f| f.only_selected).to_string()
                                            on:click=toggle_only_selected
                                        >
                                            <Icon name="bookmark"/>
                                            {move || format!("Nur ausgewählte ({})", selected_count.get())}
                                        </button>
                                    </div>
                                }
                            })
                    }}
                    <label class="range-row">
                        <span>
                            "Ladestand bis "
                            <b class="mono-num">{move || format!("{:.0} %", filter.with(|f| f.max_soc))}</b>
                        </span>
                        <input
                            type="range"
                            min="5"
                            max="100"
                            step="5"
                            prop:value=move || filter.with(|f| f.max_soc.to_string())
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f32>() {
                                    filter.update(|f| f.max_soc = v);
                                }
                            }
                        />
                    </label>
                    <button
                        class="btn btn-ghost btn-sm"
                        disabled=move || !filter.with(MapFilter::is_active)
                        on:click=move |_| filter.set(MapFilter::default())
                    >
                        "Zurücksetzen"
                    </button>
                </div>
            </Show>
            // Am PC: Leiste am rechten Rand, die sich nach rechts wegschieben lässt. Am Handy: Blatt von unten.
            <button
                class=move || if sheet_open.get() { "sheet-tab is-open" } else { "sheet-tab" }
                aria-label=move || if sheet_open.get() { "Geräteliste nach rechts ausblenden" } else { "Geräteliste einblenden" }
                title=move || if sheet_open.get() { "Ausblenden" } else { "Geräteliste einblenden" }
                on:click=move |_| sheet_open.update(|o| *o = !*o)
            >
                {move || view! { <Icon name=if sheet_open.get() { "chevron-right" } else { "chevron-left" }/> }}
                <span class="sheet-tab-count mono-num">{move || shown.with(Vec::len)}</span>
            </button>
            <div class=move || if sheet_open.get() { "map-sheet is-open" } else { "map-sheet" }>
                <button
                    class="sheet-handle"
                    aria-expanded=move || sheet_open.get().to_string()
                    on:click=move |_| sheet_open.update(|o| *o = !*o)
                >
                    <span class="grab" aria-hidden="true"></span>
                    <span class="sheet-title mono-num">{move || format!("{} Geräte", shown.with(Vec::len))}</span>
                    {move || {
                        view! { <Icon name=if sheet_open.get() { "chevron-down" } else { "chevron-up" }/> }
                    }}
                </button>
                <div class="sheet-body">
                    <div class="sheet-tools">
                        <button class="btn btn-sm" on:click=move |_| fit_tick.update(|t| *t += 1)>"Alle zeigen"</button>
                        {move || {
                            route_url
                                .get()
                                .map(|href| {
                                    view! {
                                        <a class="btn btn-sm btn-primary" href=href target="_blank" rel="noopener">
                                            <Icon name="navigation"/>
                                            "Route zu den gemerkten"
                                        </a>
                                    }
                                })
                        }}
                        {move || {
                            let n = without_fix();
                            (n > 0).then(|| view! { <span class="muted">{missing_fix_text(n)}</span> })
                        }}
                    </div>
                    <ul class="map-list">
                        {move || {
                            let list = shown.get();
                            if list.is_empty() {
                                return view! { <li class="empty">"Kein Gerät passt zu den Filtern."</li> }.into_any();
                            }
                            list.into_iter()
                                .map(|d| {
                                    let id = d.id.clone();
                                    let class_id = d.id.clone();
                                    let details_id = d.id.clone();
                                    let pin_id = d.id.clone();
                                    let page_id = d.id.clone();
                                    let has_position = d.lat.is_some() && d.lon.is_some();
                                    view! {
                                        <li class="map-row">
                                            <button
                                                class=move || {
                                                    if focused.with(|f| f.as_deref() == Some(class_id.as_str())) { "map-item is-active" } else { "map-item" }
                                                }
                                                on:click=move |_| {
                                                    if has_position {
                                                        focused.set(Some(id.clone()));
                                                        jsutil::map_focus(&id);
                                                        if jsutil::is_narrow() {
                                                            sheet_open.set(false);
                                                        }
                                                    } else {
                                                        s.say("Dieses Gerät hat keine Position auf der Karte.");
                                                    }
                                                }
                                            >
                                                <span class=format!("dot {}", format::status_class(d.status))></span>
                                                <span class="map-item-id mono-num">{d.id.clone()}</span>
                                                <span class="muted mono-num">
                                                    {d.soc_percent.map_or_else(|| "unbekannt".to_owned(), format::percent)}
                                                </span>
                                            </button>
                                            <PinButton id=pin_id/>
                                            <span class="map-links">
                                                <button class="link map-details" on:click=move |_| s.selected.set(Some(details_id.clone()))>
                                                    "Details"
                                                </button>
                                                <DeviceLink id=page_id class="map-details">"Große Ansicht"</DeviceLink>
                                            </span>
                                        </li>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </ul>
                </div>
            </div>
            <div class="map-legend">
                {DeviceStatus::ALL
                    .iter()
                    .map(|st| {
                        view! {
                            <span class="legend-item">
                                <span class=format!("dot {}", st.as_str())></span>
                                {format::status_label(*st)}
                            </span>
                        }
                    })
                    .collect_view()}
                <span class="legend-item"><span class="dot me"></span>"Dein Standort"</span>
                <span class="legend-item"><span class="dot solid"></span>"Dozent"</span>
                <span class="legend-item"><span class="dot pale"></span>"Erfunden (blasser)"</span>
            </div>
        </div>
    }
}
