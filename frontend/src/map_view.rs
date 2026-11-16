//! Standortkarte. Auf dem Handy bleibt die Karte fast ganz frei: oben eine schmale Leiste mit Suche und Filterknopf,
//! unten ein Blatt, das sich bei Bedarf aufklappt. Nichts davon verwischt oder verdeckt die Karte.

use std::collections::BTreeSet;

use leptos::html;
use leptos::prelude::*;
use shared::{DataSource, DeviceStatus};

use crate::device::DeviceLink;
use crate::features::Feature;
use crate::gesture;
use crate::hint::Hint;
use crate::icons::Icon;
use crate::markers::{MapFilter, filter_summary, markers_json, missing_fix_text, narrow_toggle, narrowed_has, visible};
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
    // Schwarz-Weiß (die Vorgabe), farbige Straßenkarte oder Luftbild. Die Wahl bleibt in diesem Browser gespeichert.
    // Der Schlüssel ist neu ("bb-map-style"), damit die frühere Vorgabe "Karte" nicht als eigene Wahl hängen bleibt.
    let base = RwSignal::new(match jsutil::storage_get("bb-map-style").as_deref() {
        Some("sat") => "sat",
        Some("map") => "map",
        _ => "simple",
    });
    // Treffer der Suche unter dem Suchfeld, ein Tipp springt zum Node.
    let suggest = RwSignal::new(false);
    // Vor dem Öffnen der Details merkt sich die Karte, ob die Geräteliste offen war, und stellt das beim Zurück wieder her.
    let sheet_before = StoredValue::new(None::<bool>);
    // Zum Ansehen markierte Nodes (Strg und Klick auf Marker oder Listeneintrag): Name und Prozent stehen dauerhaft
    // an der Karte, und sie zoomt so weit heraus, dass alle zu sehen sind. Das ist getrennt von der Merkliste.
    let marked = RwSignal::new(BTreeSet::<String>::new());
    let last_marked = StoredValue::new(BTreeSet::<String>::new());
    // Am Handy lässt sich das Blatt am Griff nach unten zuziehen und nach oben aufziehen. Das offene Blatt hat eine
    // Standardgröße (so hoch wie sein Inhalt, bis zu einem Anteil der Karte). Zieht man den Griff weiter nach oben,
    // wird es höher (`sheet_extra` Pixel mehr), nach unten wieder kleiner und erst aus der Standardgröße heraus zu.
    let drag_from = StoredValue::new(None::<f64>);
    let drag_dy = RwSignal::new(0.0f64);
    let drag_raw = StoredValue::new(0.0f64);
    let drag_start_extra = StoredValue::new(0.0f64);
    let sheet_extra = RwSignal::new(0.0f64);
    let filter_menu = RwSignal::new(false);
    let filter_root = NodeRef::<html::Div>::new();
    crate::forward::close_on_outside_click(filter_root, filter_menu);
    // Am Handy steckt der Kartenstil hinter einem Knopf und klappt als Menü auf, damit er keinen Platz kostet.
    let style_menu = RwSignal::new(false);
    let style_root = NodeRef::<html::Div>::new();
    crate::forward::close_on_outside_click(style_root, style_menu);
    let toggle_marked = move |id: String| {
        marked.update(|m| {
            if !m.remove(&id) {
                m.insert(id);
            }
        });
    };

    // Die Karte wird im nächsten Bild aufgebaut: Der Tipp auf den Karten-Reiter reagiert sofort, die Karte
    // erscheint einen Wimpernschlag danach.
    node.on_load(move |_| {
        jsutil::next_frame(move || {
            let ok = jsutil::map_init("bb-map", move |id, multi| {
                if multi {
                    toggle_marked(id);
                    return;
                }
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
        jsutil::storage_set("bb-map-style", kind);
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
    // Markierte Nodes zeigen: nach den Markern setzen (Reihenfolge der Effekte). Der Ausschnitt passt sich nur an,
    // wenn sich die Markierung geändert hat, nicht bei jeder neuen Messung.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let now = marked.get();
        s.devices.track();
        let changed = last_marked.with_value(|old| *old != now);
        last_marked.set_value(now.clone());
        let ids: Vec<String> = now.iter().cloned().collect();
        jsutil::map_set_marked(&ids, changed && !ids.is_empty());
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
                    {[("simple", "Schwarz-Weiß"), ("map", "Karte"), ("sat", "Satellit")]
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
                <div class="map-style-wrap" node_ref=style_root>
                    <button
                        class=move || if style_menu.get() { "map-icon-btn is-on" } else { "map-icon-btn" }
                        aria-label="Kartenstil"
                        aria-expanded=move || style_menu.get().to_string()
                        on:click=move |_| style_menu.update(|o| *o = !*o)
                    >
                        <Icon name="layers"/>
                    </button>
                    <Show when=move || style_menu.get()>
                        <div class="map-style-menu surface" role="group" aria-label="Kartenstil">
                            {[("simple", "Schwarz-Weiß"), ("map", "Karte"), ("sat", "Satellit")]
                                .into_iter()
                                .map(|(kind, label)| {
                                    view! {
                                        <button
                                            class=move || if base.get() == kind { "is-on" } else { "" }
                                            aria-pressed=move || (base.get() == kind).to_string()
                                            on:click=move |_| {
                                                base.set(kind);
                                                style_menu.set(false);
                                            }
                                        >
                                            {label}
                                        </button>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </Show>
                </div>
            </div>
            <Show when=move || selecting.get()>
                <div class="map-panel map-select surface" role="status">
                    <p>
                        <strong class="mono-num">{move || selected_count.get()}</strong>
                        {move || if selected_count.get() == 1 { " Node ausgewählt" } else { " Nodes ausgewählt" }}
                    </p>
                    <div class="map-select-hint">
                        <Hint
                            end=true
                            text="Tippe auf die Marker der Nodes, bei denen du die Batterie wechseln willst. Sie leuchten auf und stehen in der Merkliste unter Wartung."
                        />
                    </div>
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
                    <Show when=move || s.has_simulation()>
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
                    </Show>
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
            <div
                class=move || if sheet_open.get() { "map-sheet is-open" } else { "map-sheet" }
                style=move || {
                    // Zusätzliche Höhe nach dem Aufziehen, und beim Zuziehen aus der Standardgröße folgt das Blatt dem Finger.
                    let extra = sheet_extra.get();
                    let dy = drag_dy.get();
                    let mut css = String::new();
                    if extra > 0.0 && sheet_open.get() {
                        css.push_str(&format!("--sheet-extra: {extra:.0}px;"));
                    }
                    if dy > 0.0 && sheet_open.get() {
                        css.push_str(&format!("transform: translateY({dy:.0}px); transition: none;"));
                    }
                    css
                }
            >
                <button
                    class="sheet-handle"
                    aria-expanded=move || sheet_open.get().to_string()
                    on:touchstart=move |ev| {
                        if let Some(t) = ev.touches().get(0) {
                            drag_from.set_value(Some(f64::from(t.client_y())));
                            drag_raw.set_value(0.0);
                            drag_start_extra.set_value(sheet_extra.get_untracked());
                            drag_dy.set(0.0);
                        }
                    }
                    on:touchmove=move |ev| {
                        if let (Some(from), Some(t)) = (drag_from.get_value(), ev.touches().get(0)) {
                            let dy = f64::from(t.client_y()) - from;
                            drag_raw.set_value(dy);
                            let start = drag_start_extra.get_value();
                            if sheet_open.get_untracked() && (start > 0.0 || dy < 0.0) {
                                // Offenes Blatt: Nach oben wächst es, nach unten schrumpft es bis zur Standardgröße.
                                let max = jsutil::viewport_height() * 0.6;
                                sheet_extra.set(gesture::sheet_extra_after_drag(start, dy, max));
                                drag_dy.set(0.0);
                            } else {
                                drag_dy.set(dy);
                            }
                        }
                    }
                    on:touchend=move |ev| {
                        let dy = drag_raw.get_value();
                        let start = drag_start_extra.get_value();
                        drag_from.set_value(None);
                        drag_raw.set_value(0.0);
                        drag_dy.set(0.0);
                        if dy.abs() > 12.0 {
                            // Ein Zug ist kein Tipp: Das Umschalten per Klick entfällt.
                            ev.prevent_default();
                            if !sheet_open.get_untracked() {
                                if dy < -56.0 {
                                    sheet_open.set(true);
                                }
                            } else if gesture::closes_sheet(start, dy) {
                                sheet_open.set(false);
                                sheet_extra.set(0.0);
                            }
                        }
                    }
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
                            // Ein Tipp zeigt in der Liste nur die Geräte ohne GPS-Fix, ein zweiter bringt alle zurück.
                            let n = without_fix();
                            let on = filter.with(|f| f.no_fix_only);
                            (n > 0 || on)
                                .then(|| {
                                    view! {
                                        <button
                                            class="link sheet-nofix"
                                            aria-pressed=on.to_string()
                                            on:click=move |_| filter.update(|f| f.no_fix_only = !f.no_fix_only)
                                        >
                                            {if on { "Zurück zu allen Geräten".to_owned() } else { missing_fix_text(n) }}
                                        </button>
                                    }
                                })
                        }}
                    </div>
                    <div class="sheet-filter" node_ref=filter_root>
                        <button
                            class=move || if active_filters() { "btn btn-sm is-on-filter" } else { "btn btn-sm" }
                            aria-expanded=move || filter_menu.get().to_string()
                            on:click=move |_| filter_menu.update(|o| *o = !*o)
                        >
                            <Icon name="filter"/>
                            {move || format!("Zeigen: {}", filter.with(filter_summary))}
                            {move || view! { <Icon name=if filter_menu.get() { "chevron-up" } else { "chevron-down" }/> }}
                        </button>
                        <Show when=move || filter_menu.get()>
                            <div class="sheet-filter-menu" role="group" aria-label="Geräte filtern">
                                <span class="sheet-filter-title">"Zustand"</span>
                                <div class="chips">
                                    {DeviceStatus::ALL
                                        .iter()
                                        .map(|st| {
                                            let st = *st;
                                            view! {
                                                <button
                                                    class=move || {
                                                        if filter.with(|f| narrowed_has(&f.statuses, DeviceStatus::ALL, &st)) { "chip is-active" } else { "chip" }
                                                    }
                                                    aria-pressed=move || {
                                                        filter.with(|f| narrowed_has(&f.statuses, DeviceStatus::ALL, &st)).to_string()
                                                    }
                                                    on:click=move |_| {
                                                        filter.update(|f| narrow_toggle(&mut f.statuses, DeviceStatus::ALL, st))
                                                    }
                                                >
                                                    <span class=format!("dot {}", format::status_class(st))></span>
                                                    {format::status_label(st)}
                                                </button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                                <Show when=move || s.has_simulation()>
                                <span class="sheet-filter-title">"Herkunft"</span>
                                <div class="chips">
                                    {DataSource::ALL
                                        .iter()
                                        .map(|src| {
                                            let src = *src;
                                            view! {
                                                <button
                                                    class=move || {
                                                        if filter.with(|f| narrowed_has(&f.sources, DataSource::ALL, &src)) { "chip is-active" } else { "chip" }
                                                    }
                                                    aria-pressed=move || {
                                                        filter.with(|f| narrowed_has(&f.sources, DataSource::ALL, &src)).to_string()
                                                    }
                                                    on:click=move |_| {
                                                        filter.update(|f| narrow_toggle(&mut f.sources, DataSource::ALL, src))
                                                    }
                                                >
                                                    {format::source_label(src)}
                                                </button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                                </Show>
                                {move || {
                                    s.on(Feature::Favorites)
                                        .then(|| {
                                            view! {
                                                <span class="sheet-filter-title">"Merkliste"</span>
                                                <div class="chips">
                                                    <button
                                                        class=move || if filter.with(|f| f.only_selected) { "chip is-active" } else { "chip" }
                                                        aria-pressed=move || filter.with(|f| f.only_selected).to_string()
                                                        on:click=toggle_only_selected
                                                    >
                                                        <Icon name="bookmark"/>
                                                        {move || format!("Auf der Merkliste ({})", selected_count.get())}
                                                    </button>
                                                </div>
                                            }
                                        })
                                }}
                                <button
                                    class="btn btn-ghost btn-sm"
                                    disabled=move || !filter.with(MapFilter::is_active)
                                    on:click=move |_| filter.set(MapFilter::default())
                                >
                                    "Zurücksetzen"
                                </button>
                            </div>
                        </Show>
                    </div>
                    {move || {
                        let ids: Vec<String> = marked.get().into_iter().collect();
                        (!ids.is_empty())
                            .then(|| {
                                let text = s
                                    .devices
                                    .with(|devices| {
                                        ids.iter()
                                            .map(|id| {
                                                let soc = devices
                                                    .iter()
                                                    .find(|d| &d.id == id)
                                                    .and_then(|d| d.soc_percent)
                                                    .map_or_else(|| "unbekannt".to_owned(), format::percent);
                                                format!("{id} {soc}")
                                            })
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    });
                                let count = ids.len();
                                view! {
                                    <div class="map-marked" role="status">
                                        <p>
                                            <strong class="mono-num">{count}</strong>
                                            {if count == 1 { " Node markiert: " } else { " Nodes markiert: " }}
                                            <span class="mono-num">{text}</span>
                                        </p>
                                        <button
                                            class="btn btn-sm"
                                            on:click=move |_| {
                                                let ids: Vec<String> = marked.get_untracked().into_iter().collect();
                                                jsutil::map_set_marked(&ids, true);
                                            }
                                        >
                                            "Alle zeigen"
                                        </button>
                                        <button class="btn btn-sm" on:click=move |_| marked.set(BTreeSet::new())>
                                            "Markierung aufheben"
                                        </button>
                                    </div>
                                }
                            })
                    }}
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
                                    let row_id = d.id.clone();
                                    let mark_id = d.id.clone();
                                    view! {
                                        <li class=move || {
                                            if marked.with(|m| m.contains(&row_id)) { "map-row is-marked" } else { "map-row" }
                                        }>
                                            <button
                                                class=move || {
                                                    if focused.with(|f| f.as_deref() == Some(class_id.as_str())) { "map-item is-active" } else { "map-item" }
                                                }
                                                on:click=move |ev| {
                                                    if ev.ctrl_key() || ev.meta_key() {
                                                        if has_position {
                                                            toggle_marked(mark_id.clone());
                                                        } else {
                                                            s.say("Dieses Gerät hat keine Position auf der Karte.");
                                                        }
                                                    } else if has_position {
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
                <Show when=move || s.has_simulation()>
                    <span class="legend-item"><span class="dot ring-dozent"></span>"Dozent: weißer Rand"</span>
                    <span class="legend-item"><span class="dot ring-sim"></span>"Erfunden: dunkler Rand"</span>
                </Show>
            </div>
        </div>
    }
}
