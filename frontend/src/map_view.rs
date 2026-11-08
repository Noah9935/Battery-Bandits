//! Standortkarte mit Filter-Seitenleiste (Suche, Status, Herkunft, Ladestand) und ruhigen farbigen Punkten.

use leptos::html;
use leptos::prelude::*;
use shared::{DataSource, DeviceStatus};

use crate::markers::{MapFilter, markers_json, missing_fix_text, visible};
use crate::state::Store;
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

    node.on_load(move |_| {
        let ok = jsutil::map_init("bb-map", move |id| {
            focused.set(Some(id.clone()));
            s.selected.set(Some(id));
        });
        ready.set(ok);
    });
    on_cleanup(jsutil::map_destroy);

    // Marker bei jeder Änderung von Geräten oder Filter neu setzen. Der Ausschnitt passt sich beim ersten Mal,
    // nach einem Klick auf "Alle zeigen" und nach jeder Filteränderung an, damit man das Gefilterte sieht.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let f = filter.get();
        let tick = fit_tick.get();
        let json = s.devices.with(|devices| markers_json(&visible(devices, &f)));
        let fit = first.get_value() && json != "[]" || tick != last_fit.get_value();
        if json != "[]" {
            first.set_value(false);
        }
        last_fit.set_value(tick);
        jsutil::map_set_markers(&json, fit);
    });

    let shown = Memo::new(move |_| {
        let f = filter.get();
        s.devices
            .with(|devices| visible(devices, &f).into_iter().cloned().collect::<Vec<_>>())
    });
    let without_fix = move || shown.with(|list| list.iter().filter(|d| d.lat.is_none() || d.lon.is_none()).count());

    view! {
        <header class="page-head">
            <h1>"Standortkarte"</h1>
            <p class="sub">"Position der TrafficNodes. Mit den Filtern zeigst du nur, was du sehen willst."</p>
        </header>
        <div class="map-layout">
            <aside class="panel map-side">
                <div class="map-ctl">
                    <input
                        type="search"
                        placeholder="Gerät suchen"
                        aria-label="Gerät auf der Karte suchen"
                        prop:value=move || filter.with(|f| f.search.clone())
                        on:input=move |ev| filter.update(|f| f.search = event_target_value(&ev))
                    />
                    <div class="ctl-label">"Status"</div>
                    <div class="chips">
                        {DeviceStatus::ALL
                            .iter()
                            .map(|st| {
                                let st = *st;
                                view! {
                                    <button
                                        class=move || if filter.with(|f| f.statuses.contains(&st)) { "chip active" } else { "chip" }
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
                    <div class="ctl-label">"Herkunft"</div>
                    <div class="chips">
                        {DataSource::ALL
                            .iter()
                            .map(|src| {
                                let src = *src;
                                view! {
                                    <button
                                        class=move || if filter.with(|f| f.sources.contains(&src)) { "chip active" } else { "chip" }
                                        aria-pressed=move || filter.with(|f| f.sources.contains(&src)).to_string()
                                        on:click=move |_| toggle(filter, src, |f| &mut f.sources)
                                    >
                                        {format::source_label(src)}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                    <label class="range-row">
                        <span>
                            "Ladestand bis "
                            <b class="tabular">{move || format!("{:.0} %", filter.with(|f| f.max_soc))}</b>
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
                        class="btn btn-ghost"
                        disabled=move || !filter.with(MapFilter::is_active)
                        on:click=move |_| filter.set(MapFilter::default())
                    >
                        "Filter zurücksetzen"
                    </button>
                </div>
                <div class="panel-head">
                    <span class="tabular">{move || format!("{} Geräte", shown.with(Vec::len))}</span>
                    <button class="link" on:click=move |_| fit_tick.update(|t| *t += 1)>"Alle zeigen"</button>
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
                                let has_position = d.lat.is_some() && d.lon.is_some();
                                view! {
                                    <li class="map-row">
                                        <button
                                            class=move || {
                                                if focused.with(|f| f.as_deref() == Some(class_id.as_str())) { "map-item active" } else { "map-item" }
                                            }
                                            on:click=move |_| {
                                                if has_position {
                                                    focused.set(Some(id.clone()));
                                                    jsutil::map_focus(&id);
                                                } else {
                                                    s.say("Dieses Gerät hat keine Position auf der Karte.");
                                                }
                                            }
                                        >
                                            <span class=format!("dot {}", format::status_class(d.status))></span>
                                            <span class="map-item-id">{d.id.clone()}</span>
                                            <span class="muted tabular">
                                                {d.soc_percent.map_or_else(|| "unbekannt".to_owned(), format::percent)}
                                            </span>
                                            <span class=format!("origin {}", d.data_source.as_str())>
                                                {format::source_label(d.data_source)}
                                            </span>
                                        </button>
                                        <button class="link map-details" on:click=move |_| s.selected.set(Some(details_id.clone()))>
                                            "Details"
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
            </aside>
            <div class="panel map-main">
                <div id="bb-map" class="map" node_ref=node role="region" aria-label="Karte der TrafficNodes"></div>
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
                    <span class="legend-item"><span class="dot solid"></span>"Dozent"</span>
                    <span class="legend-item"><span class="dot pale"></span>"Erfunden (blasser)"</span>
                    {move || {
                        let n = without_fix();
                        (n > 0).then(|| view! { <span class="muted">{missing_fix_text(n)}</span> })
                    }}
                </div>
            </div>
        </div>
    }
}
