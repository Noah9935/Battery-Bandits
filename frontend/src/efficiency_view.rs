//! Effizienz: Wie viel leistet eine Batterie wirklich, verglichen mit ihrer Nennkapazität?
//!
//! Die Vergleichsseite reiht die Geräte nach einer wählbaren Kategorie, die Sektion im Gerät zeigt Kennzahlen,
//! den Einfluss von Kälte und Last und den Vergleich mit früheren Batteriezyklen. Alle Werte sind
//! Schätzungen aus Strom und Spannung, siehe `shared::efficiency`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::efficiency::{
    CycleSummary, DeviceEfficiency, EfficiencyDetail, LoadSplit, ModelSummary, REFERENCE_CURRENT_MA, Reason,
    TemperatureBand, band_label, cold_effect_mv, cost_per_day_cents, drop_share_pct, health_vs_previous,
    load_effect_mv, resistance_label, score_label, voltage_drop_mv,
};

use crate::device::{DeviceLink, Figure};
use crate::overview::{SimBadge, StatusDot};
use crate::pager::Pager;
use crate::state::Store;
use crate::{api, format, jsutil};

/// Mindestzahl Messwerte je Bereich, damit ein Vergleich etwas aussagt.
pub const MIN_READINGS: i64 = 10;

pub fn score_text(score: Option<f32>) -> String {
    score.map_or_else(
        || "keine Aussage".to_owned(),
        |s| format!("{} %", format::number(f64::from(s), 0)),
    )
}

/// Stufe der Effizienz in Worten. Sie ist immer eine Schätzung aus den Messwerten und sagt das auch.
pub fn score_note(score: Option<f32>) -> String {
    score.map_or_else(
        || "keine Aussage möglich".to_owned(),
        |s| format!("{} (Schätzung)", score_label(s)),
    )
}

/// Hinweis zur Grundlage aller Effizienz-Werte.
pub const BASIS_NOTE: &str = "Alle Werte sind nur Schätzungen.";

/// Was eine Preis-Eingabe betrifft: die eingebaute Batterie eines Geräts oder ein abgeschlossener Zyklus.
#[derive(Clone, PartialEq)]
pub enum PriceTarget {
    Device(String),
    Cycle(i64),
}

/// Preis nachtragen oder ändern: ein kleiner Knopf, der ein Feld mit Speichern öffnet. Leer entfernt den Preis.
#[component]
pub fn PriceEdit(target: PriceTarget, current: Option<i32>, on_saved: Callback<()>) -> impl IntoView {
    let s = expect_context::<Store>();
    let open = RwSignal::new(false);
    let text = RwSignal::new(
        current
            .map(|c| format::number(f64::from(c) / 100.0, 2))
            .unwrap_or_default(),
    );
    let problem = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let save = {
        let target = target.clone();
        move |_| {
            if busy.get_untracked() {
                return;
            }
            let cents = match crate::swap::parse_price_cents(&text.get_untracked()) {
                Ok(c) => c,
                Err(e) => return problem.set(Some(e.to_owned())),
            };
            problem.set(None);
            busy.set(true);
            let target = target.clone();
            spawn_local(async move {
                let result = match target {
                    PriceTarget::Device(id) => api::set_battery_price(&id, cents).await,
                    PriceTarget::Cycle(id) => api::set_cycle_price(id, cents).await,
                };
                match result {
                    Ok(_) => {
                        open.set(false);
                        s.say(if cents.is_some() {
                            "Preis gespeichert."
                        } else {
                            "Preis entfernt."
                        });
                        on_saved.run(());
                    }
                    Err(e) => problem.set(Some(e)),
                }
                busy.set(false);
            });
        }
    };
    view! {
        <span class="price-edit">
            <Show
                when=move || open.get()
                fallback=move || {
                    view! {
                        <button class="link" on:click=move |_| open.set(true)>
                            {if current.is_some() { "Preis ändern" } else { "Preis eintragen" }}
                        </button>
                    }
                }
            >
                <input
                    type="text"
                    inputmode="decimal"
                    class="price-input"
                    placeholder="89,50"
                    aria-label="Preis der Batterie in Euro"
                    prop:value=move || text.get()
                    on:input=move |ev| text.set(event_target_value(&ev))
                />
                <button class="btn btn-sm btn-primary" disabled=move || busy.get() on:click=save.clone()>
                    "Speichern"
                </button>
                <button class="btn btn-sm" on:click=move |_| open.set(false)>"Abbrechen"</button>
                {move || problem.get().map(|p| view! { <span class="form-error" role="alert">{p}</span> })}
            </Show>
        </span>
    }
}

/// Satz unter einer fehlenden Effizienz: Warum es keine Aussage gibt. Leer, wenn es eine gibt.
pub fn reason_text(reason: Option<Reason>) -> &'static str {
    reason.map_or("", Reason::text)
}

pub fn drop_text(mv_per_day: Option<f64>) -> String {
    match mv_per_day {
        None => "unbekannt".to_owned(),
        Some(mv) if mv < 0.0 => format!("steigt um {} mV pro Tag", format::number(-mv, 0)),
        Some(mv) => format!("{} mV pro Tag", format::number(mv, 0)),
    }
}

pub fn runtime_text(hours: Option<f64>) -> String {
    hours.map_or_else(|| "unbekannt".to_owned(), format::runtime)
}

pub fn current_text(ma: Option<f64>) -> String {
    ma.map_or_else(|| "unbekannt".to_owned(), |ma| format!("{} mA", format::number(ma, 0)))
}

/// Wonach sich die Geräte vergleichen lassen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Gesamtbild: reale gegen nominale Laufzeit.
    Efficiency,
    /// Wie lange die Batterie insgesamt hält (reale Laufzeit).
    Durability,
    /// Kosten je Tag, aus dem nachgetragenen Preis.
    Price,
    /// Wie viel Last die Batterie ohne Spannungseinbruch trägt.
    Power,
    /// Wie schnell die Spannung sinkt.
    Drop,
    /// Wie viel Strom das Gerät im Mittel zieht.
    Current,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Efficiency,
        Category::Durability,
        Category::Price,
        Category::Power,
        Category::Drop,
        Category::Current,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Category::Efficiency => "efficiency",
            Category::Durability => "durability",
            Category::Price => "price",
            Category::Power => "power",
            Category::Drop => "drop",
            Category::Current => "current",
        }
    }

    pub fn parse(key: &str) -> Category {
        Category::ALL
            .into_iter()
            .find(|c| c.key() == key)
            .unwrap_or(Category::Efficiency)
    }

    /// Name in der Auswahl.
    pub fn label(self) -> &'static str {
        match self {
            Category::Efficiency => "Effizienz (Schätzung)",
            Category::Durability => "Haltbarkeit (reale Laufzeit)",
            Category::Price => "Preis (Kosten pro Tag)",
            Category::Power => "Leistung (Innenwiderstand)",
            Category::Drop => "Spannungsabfall pro Tag",
            Category::Current => "Stromverbrauch",
        }
    }

    /// Ob ein größerer Wert besser ist. Bei Preis, Widerstand, Abfall und Strom ist ein kleiner Wert besser.
    pub fn high_is_better(self) -> bool {
        matches!(self, Category::Efficiency | Category::Durability)
    }

    pub fn value(self, d: &DeviceEfficiency) -> Option<f64> {
        match self {
            Category::Efficiency => d.result.score_pct.map(f64::from),
            Category::Durability => d.result.real_runtime_hours,
            Category::Price => cost_per_day_cents(d.price_cents, d.battery_hours.unwrap_or(0.0)),
            Category::Power => d.internal_resistance_mohm,
            Category::Drop => d.result.voltage_drop_mv_per_day,
            Category::Current => d.result.avg_current_ma,
        }
    }

    /// Der Wert so, wie er in der Zeile steht. Ohne Wert steht da, woran es fehlt.
    pub fn text(self, d: &DeviceEfficiency) -> String {
        let Some(v) = self.value(d) else {
            return match self {
                Category::Price if d.price_cents.is_none() => "kein Preis eingetragen".to_owned(),
                Category::Price => "zu kurz im Einsatz".to_owned(),
                _ => "keine Aussage".to_owned(),
            };
        };
        match self {
            Category::Efficiency => score_text(d.result.score_pct),
            Category::Durability => runtime_text(Some(v)),
            Category::Price => format!("{} pro Tag", euro_or_none(Some(v))),
            Category::Power => format!("{} mΩ ({})", format::number(v, 1), resistance_label(v)),
            Category::Drop => drop_text(Some(v)),
            Category::Current => current_text(Some(v)),
        }
    }
}

/// Geräte nach einer Kategorie sortiert. Geräte ohne Wert stehen immer am Ende, gleiche Werte nach Kennung.
pub fn sorted_by(list: &[DeviceEfficiency], category: Category, best_first: bool) -> Vec<DeviceEfficiency> {
    let mut sorted = list.to_vec();
    let ascending = category.high_is_better() != best_first;
    sorted.sort_by(|a, b| {
        let by_value = match (category.value(a), category.value(b)) {
            (Some(x), Some(y)) if ascending => x.total_cmp(&y),
            (Some(x), Some(y)) => y.total_cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        by_value.then_with(|| a.device_id.cmp(&b.device_id))
    });
    sorted
}

/// Geräte je Seite auf der Effizienz-Seite (jede Zeile ist hoch, deshalb weniger als in der Geräteliste).
pub const EFFICIENCY_PAGE_SIZE: usize = 10;

/// Geräte, die zur Suche (Kennung oder Name) und zur Stufe passen. Leere Suche und leere Stufe lassen alles durch.
pub fn filter_efficiency(list: &[DeviceEfficiency], query: &str, level: &str) -> Vec<DeviceEfficiency> {
    let needle = query.trim().to_lowercase();
    list.iter()
        .filter(|d| {
            needle.is_empty()
                || d.device_id.to_lowercase().contains(&needle)
                || d.name.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle))
        })
        .filter(|d| match (level, d.result.score_pct) {
            ("", _) => true,
            ("keine", score) => score.is_none(),
            (wanted, Some(score)) => score_label(score) == wanted,
            (_, None) => false,
        })
        .cloned()
        .collect()
}

/// Mittlere Effizienz aller Geräte, für die es eine Aussage gibt.
pub fn fleet_average(list: &[DeviceEfficiency]) -> Option<f32> {
    let scores: Vec<f32> = list.iter().filter_map(|d| d.result.score_pct).collect();
    (!scores.is_empty()).then(|| scores.iter().sum::<f32>() / scores.len() as f32)
}

/// Einfluss der Kälte in einem Satz.
pub fn cold_text(bands: &[TemperatureBand]) -> String {
    match cold_effect_mv(bands, MIN_READINGS) {
        Some(mv) if mv >= 50.0 => format!(
            "Bei Kälte liegt die Spannung im Mittel {} mV tiefer als in den warmen Stunden. Kälte kostet hier Leistung.",
            format::number(mv, 0)
        ),
        Some(mv) if mv <= -50.0 => "Bei Kälte liegt die Spannung nicht tiefer als bei Wärme.".to_owned(),
        Some(_) => "Kein spürbarer Einfluss der Temperatur auf die Spannung.".to_owned(),
        None => "Für einen Vergleich fehlen Messwerte aus mindestens zwei Temperaturbereichen.".to_owned(),
    }
}

/// Einfluss hoher Last (Senden) in einem Satz.
pub fn load_text(load: Option<&LoadSplit>) -> String {
    let Some(load) = load else {
        return "Noch keine Messwerte für einen Lastvergleich.".to_owned();
    };
    match load_effect_mv(load, MIN_READINGS) {
        Some(mv) if mv >= 50.0 => format!(
            "Unter hoher Last (mehr als das {}fache des üblichen Stroms von {} mA) fällt die Spannung im Mittel um {} mV.",
            format::number(shared::efficiency::HIGH_LOAD_FACTOR, 1),
            format::number(load.median_current_ma, 0),
            format::number(mv, 0)
        ),
        Some(_) => "Hohe Last drückt die Spannung kaum.".to_owned(),
        None => "Zu wenige Messwerte unter hoher Last für einen Vergleich.".to_owned(),
    }
}

/// Gesundheit im Vergleich zu früheren Batterien in einem Satz.
pub fn health_text(cycles: &[CycleSummary]) -> String {
    match health_vs_previous(cycles) {
        Some(pct) => format!(
            "Die laufende Batterie erreicht {} der Effizienz früherer Batterien dieses Geräts.",
            score_text(Some(pct))
        ),
        None => "Noch kein Vergleich: Es fehlt ein abgeschlossener Zyklus mit Messwerten.".to_owned(),
    }
}

/// Betrag in Euro aus Cent als Kommazahl, `keine Aussage` ohne Wert.
pub fn euro_or_none(cents: Option<f64>) -> String {
    cents.map_or_else(|| "keine Aussage".to_owned(), |c| shared::euro_text(c.round() as i32))
}

/// Die Kennzahlen eines Modells als Zeilen für die Liste: wie lange, wie viel, was es kostet.
pub fn model_facts(m: &ModelSummary) -> Vec<String> {
    let per_day = m.ah_per_day.map_or_else(
        || "keine Aussage".to_owned(),
        |a| format!("{} Ah pro Tag", format::number(a, 1)),
    );
    vec![
        format!(
            "Ø Laufzeit: {}",
            m.avg_hours.map_or_else(|| "keine Aussage".to_owned(), format::runtime)
        ),
        format!("Ø Verbrauch: {per_day}"),
        format!(
            "Ø Preis: {}",
            m.avg_price_cents
                .map_or_else(|| "nicht eingetragen".to_owned(), shared::euro_text)
        ),
        format!("Kosten je Ah: {}", euro_or_none(m.cost_per_ah_cents)),
        format!(
            "Ø Innenwiderstand: {}",
            m.avg_resistance_mohm.map_or_else(
                || "keine Aussage".to_owned(),
                |r| format!("{} mΩ ({})", format::number(r, 1), resistance_label(r))
            )
        ),
        format!("Ø Effizienz: {}", score_text(m.avg_score_pct)),
    ]
}

/// Überschrift einer Modellzeile: `3 Batterien, 2 abgeschlossen`.
pub fn model_count_text(m: &ModelSummary) -> String {
    let word = if m.batteries == 1 { "Batterie" } else { "Batterien" };
    format!("{} {word}, {} abgeschlossen", m.batteries, m.finished)
}

/// Spannungsabfall unter Last, so wie man ihn im Alltag rechnet (`U = I * R`): Aus dem Innenwiderstand folgt, wie viel
/// Klemmenspannung bei dem mittleren Strom und beim Senden mit 5 A verloren geht.
pub fn drop_calc_text(resistance_mohm: Option<f64>, avg_current_ma: Option<f64>, terminal_mv: f64) -> String {
    let Some(r) = resistance_mohm else {
        return "Innenwiderstand: keine Aussage. Dafür muss der Strom unterschiedlich stark sein (Ruhe und Senden) und es braucht mindestens 100 Messwerte.".to_owned();
    };
    let at_send = voltage_drop_mv(r, REFERENCE_CURRENT_MA);
    let share = drop_share_pct(at_send, terminal_mv).map_or_else(String::new, |p| {
        format!(" (ein Anteil von {} Prozent der Klemmenspannung)", format::number(p, 1))
    });
    let at_avg = avg_current_ma.map_or_else(String::new, |ma| {
        format!(
            " Beim mittleren Strom von {} fällt die Klemmenspannung um {}.",
            current_text(Some(ma)),
            drop_text_mv(voltage_drop_mv(r, ma))
        )
    });
    format!(
        "Innenwiderstand etwa {} mΩ, das ist {}. U = I mal R:{at_avg} Beim Senden mit 5 A wären es {}{share}.",
        format::number(r, 1),
        resistance_label(r),
        drop_text_mv(at_send)
    )
}

fn drop_text_mv(mv: f64) -> String {
    if mv < 10.0 {
        format!("{} mV", format::number(mv, 1))
    } else {
        format!("{} mV", format::number(mv, 0))
    }
}

/// Modell und Preis einer Batterie in einer Zeile, `None` ohne beides.
pub fn battery_line(model: Option<&str>, price_cents: Option<i32>, hours: f64) -> Option<String> {
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    match (model, price_text(price_cents, hours)) {
        (None, None) => None,
        (Some(m), None) => Some(m.to_owned()),
        (None, Some(p)) => Some(p),
        (Some(m), Some(p)) => Some(format!("{m}, {p}")),
    }
}

/// Preis und Kosten je Tag einer Batterie in einer Zeile, `None` ohne Preis.
pub fn price_text(price_cents: Option<i32>, hours: f64) -> Option<String> {
    let price = price_cents?;
    Some(match cost_per_day_cents(price_cents, hours) {
        Some(per_day) => format!(
            "{}, {} pro Tag",
            shared::euro_text(price),
            shared::euro_text(per_day.round() as i32)
        ),
        None => shared::euro_text(price),
    })
}

/// Anteil der Spannungsspanne (leer bis voll) für den Balken.
pub fn voltage_share(mv: f64, min_mv: i32, max_mv: i32) -> f64 {
    let span = f64::from(max_mv) - f64::from(min_mv);
    if span <= 0.0 {
        return 0.0;
    }
    ((mv - f64::from(min_mv)) / span * 100.0).clamp(0.0, 100.0)
}

/// Sätze zur Auslastung der Datenbank. Erster Teil: wie viel belegt ist, zweiter: wohin das führt.
/// Die zweite Zeile ist leer, solange zu wenig Messwerte für eine Aussage da sind.
pub fn storage_lines(stock: &shared::efficiency::DataStock) -> (String, String, bool) {
    let used = format::bytes(stock.db_bytes as f64);
    let quota = format::bytes(stock.quota_bytes as f64);
    let first = match stock.used_pct() {
        Some(pct) if stock.quota_is_disk => format!(
            "{used} belegt ({} Prozent) von {quota}, so viel Platz hat die Datenbank auf dem Server. Davon sind {} Messwerte.",
            format::number(pct, 1),
            format::bytes(stock.readings_bytes as f64)
        ),
        Some(pct) => format!(
            "{used} von {quota} belegt ({} Prozent), davon {} Messwerte.",
            format::number(pct, 1),
            format::bytes(stock.readings_bytes as f64)
        ),
        None => format!(
            "{used} belegt, davon {} Messwerte.",
            format::bytes(stock.readings_bytes as f64)
        ),
    };
    let (Some(growth), Some(projected)) = (stock.growth_bytes_per_day(), stock.projected_bytes()) else {
        return (
            first,
            "Für eine Prognose fehlen noch Messwerte aus mindestens einem Tag.".to_owned(),
            false,
        );
    };
    let too_big = stock.quota_bytes > 0 && projected > stock.quota_bytes as f64;
    let mut second = format!(
        "Wächst um etwa {} pro Tag, nach zwölf Monaten auf etwa {}.",
        format::bytes(growth),
        format::bytes(projected),
    );
    if too_big && let Some(months) = stock.months_until_full() {
        second.push_str(&format!(" Voll voraussichtlich {}.", full_in(months)));
    }
    (first, second, too_big)
}

/// "in etwa 7 Monaten": wann der Platz voll ist, in Monaten gesagt.
pub fn full_in(months: f64) -> String {
    if months <= 0.0 {
        "schon jetzt".to_owned()
    } else if months < 1.0 {
        "in weniger als einem Monat".to_owned()
    } else if months < 1.5 {
        "in etwa einem Monat".to_owned()
    } else {
        format!("in etwa {} Monaten", months.round() as i64)
    }
}

/// Angaben in den Sätzen zur Datenbank, die dick geschrieben werden: das, was man schnell sehen will.
pub fn storage_strong(stock: &shared::efficiency::DataStock) -> Vec<String> {
    let mut strong = vec![
        format::bytes(stock.db_bytes as f64),
        format::bytes(stock.quota_bytes as f64),
        "zwölf Monate".to_owned(),
    ];
    if let Some(months) = stock.months_until_full() {
        strong.push(full_in(months));
    }
    if let Some(growth) = stock.growth_bytes_per_day() {
        strong.push(format!("{} pro Tag", format::bytes(growth)));
    }
    if let Some(projected) = stock.projected_bytes() {
        strong.push(format!("etwa {}", format::bytes(projected)));
    }
    strong
}

/// Text mit dick geschriebenen Angaben.
fn bold_parts(text: &str, strong: &[String]) -> Vec<(String, bool)> {
    let refs: Vec<&str> = strong.iter().map(String::as_str).collect();
    crate::nodes::emphasize(text, &refs)
}

/// Balken mit der Auslastung der Datenbank und der Prognose.
#[component]
fn StorageBar(stock: shared::efficiency::DataStock) -> impl IntoView {
    let pct = stock.used_pct().unwrap_or(0.0).clamp(0.0, 100.0);
    let (first, second, too_big) = storage_lines(&stock);
    let strong = storage_strong(&stock);
    let (first_parts, second_parts) = (bold_parts(&first, &strong), bold_parts(&second, &strong));
    let to_view = |parts: Vec<(String, bool)>| {
        parts
            .into_iter()
            .map(|(part, bold)| {
                if bold {
                    view! { <strong>{part}</strong> }.into_any()
                } else {
                    part.into_any()
                }
            })
            .collect_view()
    };
    let high = pct >= 80.0 || too_big;
    view! {
        <div class=if high { "storage storage-high" } else { "storage" }>
            <div class="storage-head">
                <strong>"Speicher der Datenbank"</strong>
                <span class="mono-num">{format!("{} %", format::number(pct, 1))}</span>
            </div>
            <div class="eff-bar" role="img" aria-label=format!("Datenbank zu {pct:.0} Prozent belegt")>
                <div class="eff-fill" style=format!("width: {pct:.1}%")></div>
            </div>
            <p class="muted">{to_view(first_parts)}</p>
            <p class=if too_big { "form-error" } else { "muted" }>{to_view(second_parts)}</p>
        </div>
    }
}

/// Nachrichten je Seite im Protokoll der verworfenen Nachrichten.
pub const INVALID_PAGE_SIZE: usize = 10;

/// Protokoll der verworfenen Nachrichten: Zeit, Thema, Grund und der Inhalt, seitenweise, neueste zuerst.
#[component]
pub fn InvalidLog() -> impl IntoView {
    let page = RwSignal::new(0usize);
    let data = RwSignal::new(None::<shared::api::InvalidMessagesPage>);
    let error = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        let offset = page.get() * INVALID_PAGE_SIZE;
        spawn_local(async move {
            match api::invalid_messages(INVALID_PAGE_SIZE, offset).await {
                Ok(p) => {
                    error.set(None);
                    data.set(Some(p));
                }
                Err(e) => error.set(Some(e)),
            }
        });
    });
    let total = Signal::derive(move || data.with(|d| d.as_ref().map_or(0, |d| usize::try_from(d.total).unwrap_or(0))));
    view! {
        <div id="invalid-log" class="invalid-log">
            <Pager
                total=total
                page=page
                size=INVALID_PAGE_SIZE
                label="Seiten der verworfenen Nachrichten (oben)"
                anchor="invalid-log"
                top=true
            />
            {move || error.get().map(|e| view! { <p class="form-error" role="alert">{e}</p> })}
            <ul class="invalid-list">
                {move || {
                    let Some(d) = data.get() else {
                        return view! { <li class="empty">"Das Protokoll wird geladen."</li> }.into_any();
                    };
                    if d.items.is_empty() {
                        return view! { <li class="empty">"Es wurde noch keine Nachricht verworfen."</li> }.into_any();
                    }
                    d.items
                        .into_iter()
                        .map(|m| {
                            view! {
                                <li class="invalid-row">
                                    <div class="invalid-head">
                                        <strong>{m.reason}</strong>
                                        <span class="muted mono-num">{jsutil::date_time_label(m.received_at)}</span>
                                    </div>
                                    <span class="muted mono-num">{m.topic}</span>
                                    <code class="invalid-payload">{m.payload}</code>
                                </li>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </ul>
            <Pager
                total=total
                page=page
                size=INVALID_PAGE_SIZE
                label="Seiten der verworfenen Nachrichten"
                anchor="invalid-log"
            />
        </div>
    }
}

/// Seite "Effizienz": Datenbestand und der Vergleich der Geräte nach einer wählbaren Kategorie.
#[component]
pub fn EfficiencyPage() -> impl IntoView {
    let s = expect_context::<Store>();
    // Die Seite wird erst beim ersten Öffnen aufgebaut: Dann laden.
    s.load_efficiency();
    let list = Memo::new(move |_| {
        s.efficiency
            .with(|e| e.as_ref().map(|e| e.devices.clone()).unwrap_or_default())
    });
    let average = Memo::new(move |_| {
        s.efficiency
            .with(|e| e.as_ref().and_then(|e| fleet_average(&e.devices)))
    });
    // Suche und Seite: Es wird immer nur eine Seite der gefilterten Liste gezeigt.
    let query = RwSignal::new(String::new());
    let category = RwSignal::new(Category::Efficiency);
    // Anfangs das schwächste Gerät zuerst: Das ist, was man zuerst sehen will.
    let best_first = RwSignal::new(false);
    let page = RwSignal::new(0usize);
    let filtered = Memo::new(move |_| {
        let q = query.get();
        // Die Auswahl nach Stufe ist fürs Erste aus der Oberfläche genommen, die Filterfunktion kann sie noch.
        let rows = list.with(|rows| filter_efficiency(rows, &q, ""));
        sorted_by(&rows, category.get(), best_first.get())
    });
    let shown = Signal::derive(move || filtered.with(Vec::len));
    let all = Signal::derive(move || list.with(Vec::len));
    Effect::new(move |_| {
        query.track();
        category.track();
        best_first.track();
        page.set(0);
    });
    let page_rows = Memo::new(move |_| {
        let (start, end, _) = crate::filter::page_bounds(filtered.with(Vec::len), page.get(), EFFICIENCY_PAGE_SIZE);
        filtered.with(|rows| rows[start..end].to_vec())
    });
    let invalid_open = RwSignal::new(false);
    view! {
        <div class="view">
            <header class="view-head">
                <h1>"Effizienz"</h1>
            </header>
            {move || s.efficiency_error.get().map(|e| view! { <p class="form-error" role="alert">{e}</p> })}
            <section id="eff-stock" class="surface stock">
                <div class="surface-head">
                    <h2>"Datenhaltung"</h2>
                </div>
                <div class="surface-body stock-grid">
                    {move || {
                        s.efficiency
                            .get()
                            .map(|e| {
                                let stock = e.stock;
                                view! {
                                    <Figure label="Gespeicherte Messwerte" value=format::number(stock.readings_total as f64, 0)/>
                                    <Figure
                                        label="Ältester Messwert"
                                        value=stock.oldest_reading.map_or_else(|| "noch keiner".to_owned(), jsutil::date_label)
                                    />
                                    <Figure
                                        label="Neuester Messwert"
                                        value=stock.newest_reading.map_or_else(|| "noch keiner".to_owned(), jsutil::date_time_label)
                                    />
                                    <Figure
                                        label="Verworfene Nachrichten"
                                        value=format::number(stock.invalid_messages as f64, 0)
                                    />
                                    <button
                                        class="btn figure-btn"
                                        aria-expanded=move || invalid_open.get().to_string()
                                        on:click=move |_| invalid_open.update(|o| *o = !*o)
                                    >
                                        {move || if invalid_open.get() { "Protokoll schließen" } else { "mit Grund protokolliert" }}
                                    </button>
                                }
                            })
                    }}
                </div>
                {move || s.efficiency.with(|e| e.as_ref().map(|e| e.stock.clone())).map(|stock| view! { <StorageBar stock=stock/> })}
                <Show when=move || invalid_open.get()>
                    <InvalidLog/>
                </Show>
            </section>
            <ModelSection/>
            <section id="eff-compare" class="surface">
                <div class="surface-head">
                    <h2>"Geräte im Vergleich"</h2>
                    <span class="muted mono-num">
                        {move || {
                            average.get().map_or_else(
                                || "noch keine Aussage".to_owned(),
                                |a| format!("Mittel {}", score_text(Some(a))),
                            )
                        }}
                    </span>
                </div>
                <div id="eff-toolbar" class="eff-toolbar" role="search" aria-label="Filter der Effizienz">
                    <input
                        type="search"
                        class="eff-search"
                        placeholder="Gerät suchen"
                        aria-label="Gerät suchen"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                    <select
                        class="eff-level"
                        aria-label="Vergleichen nach"
                        on:change=move |ev| category.set(Category::parse(&event_target_value(&ev)))
                    >
                        {Category::ALL
                            .into_iter()
                            .map(|c| {
                                view! {
                                    <option value=c.key() selected=move || category.get() == c>
                                        {c.label()}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                    <button
                        class="btn"
                        aria-label="Reihenfolge umdrehen"
                        on:click=move |_| best_first.update(|b| *b = !*b)
                    >
                        {move || if best_first.get() { "Beste zuerst" } else { "Schwächste zuerst" }}
                    </button>
                    <span class="muted mono-num eff-count">
                        {move || format!("{} von {} Geräten", shown.get(), all.get())}
                    </span>
                </div>
                <Pager
                    total=shown
                    page=page
                    size=EFFICIENCY_PAGE_SIZE
                    label="Seiten der Effizienz (oben)"
                    anchor="eff-toolbar"
                    top=true
                />
                <ul class="eff-list">
                    {move || {
                        let rows = page_rows.get();
                        if rows.is_empty() {
                            let text = if all.get() == 0 {
                                "Noch keine Geräte mit Messwerten."
                            } else {
                                "Kein Gerät passt zu Suche und Stufe."
                            };
                            return view! { <li class="empty">{text}</li> }.into_any();
                        }
                        let category = category.get();
                        rows.into_iter()
                            .map(|d| {
                                let share = d.result.score_pct.map_or(0.0, |v| f64::from(v).min(100.0));
                                let badge_id = d.device_id.clone();
                                let title = format::device_title(&d.device_id, d.name.as_deref());
                                view! {
                                    <li class="eff-row">
                                        <div class="eff-main">
                                            <DeviceLink id=d.device_id.clone() class="eff-id mono-num">
                                                {title}
                                            </DeviceLink>
                                            <StatusDot status=d.status/>
                                            <SimBadge id=badge_id/>
                                        </div>
                                        <div class="eff-score">
                                            <strong class="mono-num">{category.text(&d)}</strong>
                                            <span class="muted">
                                                {if category == Category::Efficiency {
                                                    score_note(d.result.score_pct)
                                                } else {
                                                    category.label().to_owned()
                                                }}
                                            </span>
                                        </div>
                                        <div class="eff-bar" aria-hidden="true">
                                            <div class="eff-fill" style=format!("width: {share:.0}%")></div>
                                        </div>
                                        <p class="eff-reason muted">{reason_text(d.result.reason)}</p>
                                        <div class="eff-facts muted mono-num">
                                            <span>{current_text(d.result.avg_current_ma)}</span>
                                            <span>{drop_text(d.result.voltage_drop_mv_per_day)}</span>
                                            <span>{format!("real: {}", runtime_text(d.result.real_runtime_hours))}</span>
                                            <span>{format!("nominal: {}", runtime_text(d.result.nominal_runtime_hours))}</span>
                                            <span>{format!("Preis: {}", Category::Price.text(&d))}</span>
                                            <span>{format!("Widerstand: {}", Category::Power.text(&d))}</span>
                                        </div>
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
                <Pager
                    total=shown
                    page=page
                    size=EFFICIENCY_PAGE_SIZE
                    label="Seiten der Effizienz"
                    anchor="eff-toolbar"
                />
            </section>
            <p class="legend">
                "Schätzung: Die Spannung fällt zwischen voll und leer näherungsweise linear. Gemessen wird in den letzten 30 Tagen, höchstens seit dem letzten Batteriewechsel."
            </p>
            <p class="legend">{BASIS_NOTE}</p>
        </div>
    }
}

/// Effizienz-Sektion in der Ansicht eines Geräts.
#[component]
pub fn EfficiencySection(id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let detail = RwSignal::new(None::<EfficiencyDetail>);
    let error = RwSignal::new(None::<String>);
    let load = {
        let id = id.clone();
        move || {
            let id = id.clone();
            spawn_local(async move {
                match api::device_efficiency(&id).await {
                    Ok(d) => detail.set(Some(d)),
                    Err(e) => error.set(Some(e)),
                }
            });
        }
    };
    load();
    let reload = Callback::new(move |()| load());
    let device_id_for_price = id.clone();
    let volts = move || {
        s.settings.with(|st| {
            st.as_ref()
                .map_or((10_500, 12_700), |st| (st.min_voltage_mv, st.max_voltage_mv))
        })
    };
    view! {
        <section class="surface device-eff">
            <div class="surface-head">
                <h2>"Effizienz"</h2>
            </div>
            <div class="surface-body">
                {move || error.get().map(|e| view! { <p class="empty">{e}</p> })}
                {move || {
                    let Some(d) = detail.get() else {
                        return (error.with(Option::is_none)).then(|| view! { <p class="empty">"Effizienz wird berechnet."</p> }.into_any());
                    };
                    let r = d.current.result;
                    let (min_mv, max_mv) = volts();
                    let terminal_mv = s
                        .device(&d.current.device_id)
                        .and_then(|dev| dev.voltage_mv)
                        .map_or(12_000.0, f64::from);
                    let bands = d.bands.clone();
                    Some(
                        view! {
                            <p class="legend eff-disclaimer">{BASIS_NOTE}</p>
                            <div class="figures">
                                <Figure label="Effizienz (Schätzung)" value=score_text(r.score_pct) hint=score_note(r.score_pct)/>
                                <Figure label="Mittlerer Strom" value=current_text(r.avg_current_ma)/>
                                <Figure label="Spannungsabfall" value=drop_text(r.voltage_drop_mv_per_day)/>
                                <Figure
                                    label="Laufzeit laut Nennkapazität"
                                    value=runtime_text(r.nominal_runtime_hours)
                                />
                                <Figure label="Reale Laufzeit" value=runtime_text(r.real_runtime_hours)/>
                                <Figure
                                    label="Innenwiderstand"
                                    value=d.current.internal_resistance_mohm.map_or_else(|| "keine Aussage".to_owned(), |v| format!("{} mΩ", format::number(v, 1)))
                                    hint=d.current.internal_resistance_mohm.map(|v| resistance_label(v).to_owned()).unwrap_or_default()
                                />
                            </div>
                            <h3>"Spannungsabfall unter Last"</h3>
                            <p class="forecast-text">{drop_calc_text(d.current.internal_resistance_mohm, r.avg_current_ma, terminal_mv)}</p>
                            {r.reason.map(|reason| view! { <p class="forecast-text">{reason.text()}</p> })}
                            <h3>"Temperatur"</h3>
                            <ul class="band-list">
                                {bands
                                    .iter()
                                    .map(|b| {
                                        let share = voltage_share(b.avg_voltage_mv, min_mv, max_mv);
                                        view! {
                                            <li class="band-row">
                                                <span class="band-name">{band_label(b.from_c, b.to_c)}</span>
                                                <span class="eff-bar" aria-hidden="true">
                                                    <span class="eff-fill" style=format!("width: {share:.0}%")></span>
                                                </span>
                                                <span class="mono-num">{format::voltage(b.avg_voltage_mv.round() as i32)}</span>
                                                <span class="muted mono-num">{format!("{} Messwerte", b.readings)}</span>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                            <p class="forecast-text">{cold_text(&bands)}</p>
                            <h3>"Last beim Senden"</h3>
                            <p class="forecast-text">{load_text(d.load.as_ref())}</p>
                            <h3>"Batteriezyklen"</h3>
                            <p class="forecast-text">{health_text(&d.cycles)}</p>
                            <ul class="cycle-list">
                                {d
                                    .cycles
                                    .iter()
                                    .map(|c| {
                                        let share = c.result.score_pct.map_or(0.0, |v| f64::from(v).min(100.0));
                                        let when = match c.ended_at {
                                            None => format!("Laufend seit {}", jsutil::date_label(c.started_at)),
                                            Some(end) => format!(
                                                "{} bis {}",
                                                jsutil::date_label(c.started_at),
                                                jsutil::date_label(end)
                                            ),
                                        };
                                        view! {
                                            <li class="cycle-row">
                                                <span class="band-name">{when}</span>
                                                <span class="eff-bar" aria-hidden="true">
                                                    <span class="eff-fill" style=format!("width: {share:.0}%")></span>
                                                </span>
                                                <strong class="mono-num">{score_text(c.result.score_pct)}</strong>
                                                <span class="muted mono-num">
                                                    {format!(
                                                        "{}, {} Ah",
                                                        format::runtime(c.hours),
                                                        format::number(c.discharged_ah, 1)
                                                    )}
                                                </span>
                                                {battery_line(c.model.as_deref(), c.price_cents, c.hours)
                                                    .map(|text| view! { <span class="muted mono-num cycle-price">{text}</span> })}
                                                <PriceEdit
                                                    target=match c.id {
                                                        Some(cycle_id) => PriceTarget::Cycle(cycle_id),
                                                        None => PriceTarget::Device(device_id_for_price.clone()),
                                                    }
                                                    current=c.price_cents
                                                    on_saved=reload
                                                />
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                            .into_any(),
                    )
                }}
            </div>
        </section>
    }
}

/// So viele Batteriemodelle stehen auf einer Seite.
const MODEL_PAGE_SIZE: usize = 5;

/// Die Modelle im Vergleich, mit Suche nach dem Namen und Seiten, damit nicht alles auf einmal dasteht.
#[component]
fn ModelSection() -> impl IntoView {
    let s = expect_context::<Store>();
    let query = RwSignal::new(String::new());
    let page = RwSignal::new(0usize);
    let all = Memo::new(move |_| s.efficiency.with(|e| e.as_ref().map_or(0, |e| e.models.len())));
    let found = Memo::new(move |_| {
        let q = query.get();
        s.efficiency.with(|e| {
            e.as_ref()
                .map(|e| {
                    e.models
                        .iter()
                        .filter(|m| crate::search::score(&q, &m.model).is_some())
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    });
    let total = Signal::derive(move || found.with(Vec::len));
    let rows = move || {
        let list = found.get();
        let (start, end, _) = crate::filter::page_bounds(list.len(), page.get(), MODEL_PAGE_SIZE);
        list.into_iter().skip(start).take(end - start).collect::<Vec<_>>()
    };
    view! {
        <section id="eff-models" class="surface">
            <div class="surface-head">
                <h2>"Nach Batteriemodell"</h2>
                <span class="muted">"Gleiche Modelle zusammengenommen"</span>
            </div>
            <Show when=move || { all.get() > MODEL_PAGE_SIZE }>
                <div id="model-toolbar" class="eff-toolbar" role="search" aria-label="Batteriemodell suchen">
                    <input
                        type="search"
                        class="eff-search"
                        placeholder="Modell suchen"
                        aria-label="Modell suchen"
                        prop:value=move || query.get()
                        on:input=move |ev| {
                            query.set(event_target_value(&ev));
                            page.set(0);
                        }
                    />
                    <span class="muted mono-num eff-count">{move || format!("{} von {} Modellen", total.get(), all.get())}</span>
                </div>
            </Show>
            <ul class="eff-list">
                {move || {
                    if all.get() == 0 {
                        return view! { <li class="empty">"Noch keine Batterien erfasst. Das Modell trägt man beim Batteriewechsel ein."</li> }.into_any();
                    }
                    if total.get() == 0 {
                        return view! { <li class="empty">"Kein Modell passt zur Suche."</li> }.into_any();
                    }
                    rows()
                        .into_iter()
                        .map(|m| {
                            let facts = model_facts(&m);
                            view! {
                                <li class="eff-row">
                                    <div class="eff-main">
                                        <strong class="eff-id">{m.model.clone()}</strong>
                                        <span class="muted">{model_count_text(&m)}</span>
                                    </div>
                                    <div class="eff-score">
                                        <strong class="mono-num">{euro_or_none(m.cost_per_day_cents)}</strong>
                                        <span class="muted">"pro Tag"</span>
                                    </div>
                                    <div class="eff-facts muted mono-num">
                                        {facts.into_iter().map(|f| view! { <span>{f}</span> }).collect_view()}
                                    </div>
                                </li>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </ul>
            <Pager total=total page=page size=MODEL_PAGE_SIZE label="Seiten der Batteriemodelle" anchor="model-toolbar"/>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::DeviceStatus;
    use shared::efficiency::Efficiency;

    fn eff(id: &str, score: Option<f32>) -> DeviceEfficiency {
        DeviceEfficiency {
            device_id: id.into(),
            name: None,
            status: DeviceStatus::Normal,
            window_hours: 48.0,
            readings: 100,
            avg_temperature_c: None,
            internal_resistance_mohm: None,
            price_cents: None,
            battery_hours: None,
            result: Efficiency {
                score_pct: score,
                ..Efficiency::default()
            },
        }
    }

    fn band(readings: i64, mv: f64) -> TemperatureBand {
        TemperatureBand {
            from_c: None,
            to_c: Some(0.0),
            readings,
            avg_current_ma: 300.0,
            avg_voltage_mv: mv,
        }
    }

    #[test]
    fn scores_and_notes_read_naturally() {
        assert_eq!(score_text(Some(82.4)), "82 %");
        assert_eq!(score_text(None), "keine Aussage");
        assert_eq!(score_note(Some(95.0)), "wie neu (Schätzung)");
        assert_eq!(score_note(None), "keine Aussage möglich");
        assert_eq!(reason_text(None), "");
        assert!(reason_text(Some(Reason::Implausible)).contains("120 Prozent"));
    }

    #[test]
    fn units_are_german_and_say_when_the_voltage_rises() {
        assert_eq!(drop_text(Some(528.4)), "528 mV pro Tag");
        assert_eq!(drop_text(Some(-40.0)), "steigt um 40 mV pro Tag");
        assert_eq!(drop_text(None), "unbekannt");
        assert_eq!(runtime_text(Some(36.0)), "36 Std.");
        assert_eq!(runtime_text(Some(300.0)), "12,5 Tage");
        assert_eq!(current_text(Some(812.3)), "812 mA");
        assert_eq!(current_text(None), "unbekannt");
    }

    #[test]
    fn the_weakest_battery_comes_first_and_unknown_ones_last() {
        let list = [
            eff("C", Some(90.0)),
            eff("U", None),
            eff("A", Some(40.0)),
            eff("B", Some(40.0)),
        ];
        let ids = |best_first| -> Vec<String> {
            sorted_by(&list, Category::Efficiency, best_first)
                .into_iter()
                .map(|d| d.device_id)
                .collect()
        };
        assert_eq!(ids(false), ["A", "B", "C", "U"]);
        assert_eq!(ids(true), ["C", "A", "B", "U"]);
        assert_eq!(fleet_average(&list), Some((90.0 + 40.0 + 40.0) / 3.0));
        assert_eq!(fleet_average(&[eff("U", None)]), None);
        assert_eq!(fleet_average(&[]), None);
    }

    #[test]
    fn every_category_sorts_its_best_value_first_and_unknown_ones_last() {
        let mut cheap = eff("cheap", Some(80.0));
        cheap.price_cents = Some(2_400);
        cheap.battery_hours = Some(240.0);
        let mut dear = eff("dear", Some(80.0));
        dear.price_cents = Some(9_000);
        dear.battery_hours = Some(240.0);
        let free = eff("free", Some(80.0));
        let list = [dear.clone(), free.clone(), cheap.clone()];
        let ids = |c: Category, best_first| -> Vec<String> {
            sorted_by(&list, c, best_first)
                .into_iter()
                .map(|d| d.device_id)
                .collect()
        };
        // Preis: günstig ist gut, ohne Preis steht am Ende, in beide Richtungen.
        assert_eq!(ids(Category::Price, true), ["cheap", "dear", "free"]);
        assert_eq!(ids(Category::Price, false), ["dear", "cheap", "free"]);
        assert_eq!(Category::Price.text(&cheap), "2,40 € pro Tag");
        assert_eq!(Category::Price.text(&free), "kein Preis eingetragen");

        let mut slow = eff("slow", Some(50.0));
        slow.result.voltage_drop_mv_per_day = Some(100.0);
        slow.result.real_runtime_hours = Some(500.0);
        slow.internal_resistance_mohm = Some(8.0);
        slow.result.avg_current_ma = Some(200.0);
        let mut fast = eff("fast", Some(90.0));
        fast.result.voltage_drop_mv_per_day = Some(400.0);
        fast.result.real_runtime_hours = Some(100.0);
        fast.internal_resistance_mohm = Some(40.0);
        fast.result.avg_current_ma = Some(900.0);
        let list = [fast.clone(), slow.clone(), eff("none", None)];
        let ids = |c: Category, best_first| -> Vec<String> {
            sorted_by(&list, c, best_first)
                .into_iter()
                .map(|d| d.device_id)
                .collect()
        };
        assert_eq!(ids(Category::Efficiency, true), ["fast", "slow", "none"]);
        assert_eq!(ids(Category::Durability, true), ["slow", "fast", "none"]);
        assert_eq!(ids(Category::Power, true), ["slow", "fast", "none"]);
        assert_eq!(ids(Category::Drop, true), ["slow", "fast", "none"]);
        assert_eq!(ids(Category::Current, true), ["slow", "fast", "none"]);
        assert_eq!(ids(Category::Current, false), ["fast", "slow", "none"]);
        assert_eq!(Category::Power.text(&slow), "8,0 mΩ (gut)");
        assert_eq!(Category::parse("price"), Category::Price);
        assert_eq!(Category::parse("unsinn"), Category::Efficiency);
        for c in Category::ALL {
            assert_eq!(Category::parse(c.key()), c);
            assert!(!c.label().contains('-'), "{}", c.label());
        }
    }

    #[test]
    fn the_filter_matches_name_id_and_level() {
        let mut named = eff("TN0002", Some(95.0));
        named.name = Some("Kreuzung Nord".into());
        let list = [eff("TN0001", Some(40.0)), named, eff("TN0003", None)];
        let ids = |q: &str, l: &str| -> Vec<String> {
            filter_efficiency(&list, q, l)
                .into_iter()
                .map(|d| d.device_id)
                .collect()
        };
        assert_eq!(ids("", "").len(), 3);
        assert_eq!(ids("nord", ""), ["TN0002"]);
        assert_eq!(ids("tn0003", ""), ["TN0003"]);
        assert_eq!(ids("", "wie neu"), ["TN0002"]);
        assert_eq!(ids("", "schwach"), ["TN0001"]);
        assert_eq!(ids("", "keine"), ["TN0003"]);
        assert!(ids("nord", "schwach").is_empty());
    }

    #[test]
    fn the_cold_sentence_depends_on_the_difference() {
        let cold = [
            band(40, 11_900.0),
            TemperatureBand {
                from_c: Some(20.0),
                to_c: None,
                ..band(60, 12_150.0)
            },
        ];
        assert!(cold_text(&cold).contains("250 mV tiefer"), "{}", cold_text(&cold));
        let flat = [
            band(40, 12_100.0),
            TemperatureBand {
                from_c: Some(20.0),
                to_c: None,
                ..band(60, 12_120.0)
            },
        ];
        assert_eq!(
            cold_text(&flat),
            "Kein spürbarer Einfluss der Temperatur auf die Spannung."
        );
        let reversed = [
            band(40, 12_300.0),
            TemperatureBand {
                from_c: Some(20.0),
                to_c: None,
                ..band(60, 12_100.0)
            },
        ];
        assert!(cold_text(&reversed).contains("nicht tiefer"));
        assert!(cold_text(&cold[..1]).contains("mindestens zwei"));
    }

    #[test]
    fn the_load_sentence_names_the_voltage_loss() {
        let load = LoadSplit {
            median_current_ma: 300.0,
            high_readings: 20,
            high_avg_voltage_mv: Some(11_800.0),
            normal_readings: 200,
            normal_avg_voltage_mv: Some(12_100.0),
        };
        let text = load_text(Some(&load));
        assert!(text.contains("300 mA") && text.contains("um 300 mV"), "{text}");
        assert!(load_text(None).contains("Noch keine"));
        let few = LoadSplit {
            high_readings: 2,
            ..load.clone()
        };
        assert!(load_text(Some(&few)).contains("Zu wenige"));
        let mild = LoadSplit {
            high_avg_voltage_mv: Some(12_090.0),
            ..load
        };
        assert_eq!(load_text(Some(&mild)), "Hohe Last drückt die Spannung kaum.");
    }

    #[test]
    fn health_is_compared_with_earlier_cycles() {
        let at = chrono::DateTime::from_timestamp(0, 0).unwrap();
        let cycle = |ended: bool, score: f32| CycleSummary {
            id: None,
            started_at: at,
            ended_at: ended.then_some(at),
            hours: 10.0,
            discharged_ah: 1.0,
            avg_temperature_c: None,
            price_cents: None,
            model: None,
            resistance_mohm: None,
            result: Efficiency {
                score_pct: Some(score),
                ..Efficiency::default()
            },
        };
        let text = health_text(&[cycle(false, 72.0), cycle(true, 90.0)]);
        assert_eq!(
            text,
            "Die laufende Batterie erreicht 80 % der Effizienz früherer Batterien dieses Geräts."
        );
        assert!(health_text(&[cycle(false, 72.0)]).starts_with("Noch kein Vergleich"));
    }

    #[test]
    fn a_price_is_shown_with_the_cost_per_day_once_the_run_is_long_enough() {
        assert_eq!(price_text(None, 240.0), None);
        assert_eq!(
            price_text(Some(9_600), 240.0).as_deref(),
            Some("96,00 €, 9,60 € pro Tag")
        );
        assert_eq!(price_text(Some(9_600), 6.0).as_deref(), Some("96,00 €"));
    }

    fn model(cost_per_day: Option<f64>) -> ModelSummary {
        ModelSummary {
            model: "Varta 80".into(),
            batteries: 3,
            finished: 2,
            avg_hours: Some(360.0),
            avg_discharged_ah: Some(50.0),
            ah_per_day: Some(3.33),
            avg_price_cents: Some(11_000),
            cost_per_day_cents: cost_per_day,
            cost_per_ah_cents: Some(220.0),
            avg_resistance_mohm: Some(9.4),
            avg_score_pct: Some(91.0),
        }
    }

    #[test]
    fn a_model_is_described_by_what_it_lasts_and_costs() {
        let m = model(Some(733.0));
        assert_eq!(model_count_text(&m), "3 Batterien, 2 abgeschlossen");
        assert_eq!(
            model_facts(&m),
            [
                "Ø Laufzeit: 15,0 Tage",
                "Ø Verbrauch: 3,3 Ah pro Tag",
                "Ø Preis: 110,00 €",
                "Kosten je Ah: 2,20 €",
                "Ø Innenwiderstand: 9,4 mΩ (gut)",
                "Ø Effizienz: 91 %",
            ]
        );
        assert_eq!(euro_or_none(Some(733.0)), "7,33 €");
        assert_eq!(euro_or_none(None), "keine Aussage");
        let bare = ModelSummary {
            avg_hours: None,
            ah_per_day: None,
            avg_price_cents: None,
            cost_per_ah_cents: None,
            avg_resistance_mohm: None,
            avg_score_pct: None,
            batteries: 1,
            ..model(None)
        };
        assert_eq!(model_count_text(&bare), "1 Batterie, 2 abgeschlossen");
        let facts = model_facts(&bare);
        assert!(
            facts
                .iter()
                .all(|f| f.ends_with("keine Aussage") || f.ends_with("nicht eingetragen")),
            "{facts:?}"
        );
    }

    #[test]
    fn the_voltage_drop_is_calculated_like_in_real_life() {
        // 10 mΩ: bei 306 mA fallen 3,1 mV, bei 5 A 50 mV, das sind 0,4 Prozent von 12 V.
        let text = drop_calc_text(Some(10.0), Some(306.0), 12_000.0);
        assert!(text.contains("etwa 10,0 mΩ, das ist gut"), "{text}");
        assert!(
            text.contains("von 306 mA fällt die Klemmenspannung um 3,1 mV"),
            "{text}"
        );
        assert!(
            text.contains("mit 5 A wären es 50 mV (ein Anteil von 0,4 Prozent"),
            "{text}"
        );
        let high = drop_calc_text(Some(40.0), None, 12_000.0);
        assert!(
            high.contains("das ist hoch") && !high.contains("mittleren Strom"),
            "{high}"
        );
        assert!(drop_calc_text(None, Some(306.0), 12_000.0).starts_with("Innenwiderstand: keine Aussage."));
    }

    #[test]
    fn a_battery_line_joins_model_and_price() {
        assert_eq!(battery_line(None, None, 240.0), None);
        assert_eq!(
            battery_line(Some(" Varta 80 "), None, 240.0).as_deref(),
            Some("Varta 80")
        );
        assert_eq!(
            battery_line(None, Some(9_600), 240.0).as_deref(),
            Some("96,00 €, 9,60 € pro Tag")
        );
        assert_eq!(
            battery_line(Some("Varta 80"), Some(9_600), 6.0).as_deref(),
            Some("Varta 80, 96,00 €")
        );
        assert_eq!(battery_line(Some("  "), None, 1.0), None);
    }

    #[test]
    fn the_voltage_bar_spans_empty_to_full() {
        assert_eq!(voltage_share(10_500.0, 10_500, 12_700), 0.0);
        assert_eq!(voltage_share(12_700.0, 10_500, 12_700), 100.0);
        assert_eq!(voltage_share(11_600.0, 10_500, 12_700), 50.0);
        assert_eq!(voltage_share(15_000.0, 10_500, 12_700), 100.0);
        assert_eq!(voltage_share(11_000.0, 12_000, 12_000), 0.0);
    }
}

#[cfg(test)]
mod basis_tests {
    use super::*;

    #[test]
    fn the_note_is_short_and_says_the_values_are_estimates() {
        assert!(BASIS_NOTE.contains("Schätzungen"));
        assert!(BASIS_NOTE.chars().count() < 60, "nur ein kurzer Hinweis");
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use shared::efficiency::DataStock;

    fn stock(db_mb: i64, readings_mb: i64, days: i64, quota_mb: i64) -> DataStock {
        let newest = Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap();
        DataStock {
            oldest_reading: Some(newest - chrono::Duration::days(days)),
            newest_reading: Some(newest),
            db_bytes: db_mb * 1024 * 1024,
            readings_bytes: readings_mb * 1024 * 1024,
            quota_bytes: quota_mb * 1024 * 1024,
            ..DataStock::default()
        }
    }

    #[test]
    fn with_the_real_disk_the_sentences_say_so_and_the_key_figures_are_bold() {
        let mut real = stock(200, 150, 10, 10_240);
        real.quota_is_disk = true;
        let (first, second, _) = storage_lines(&real);
        assert!(
            first.contains("so viel Platz hat die Datenbank auf dem Server"),
            "{first}"
        );
        assert!(!second.contains("passt"), "{second}");
        let strong = storage_strong(&real);
        let refs: Vec<&str> = strong.iter().map(String::as_str).collect();
        let bold: Vec<String> = crate::nodes::emphasize(&second, &refs)
            .into_iter()
            .filter(|(_, b)| *b)
            .map(|(t, _)| t)
            .collect();
        assert!(bold.contains(&"15,0 MB pro Tag".to_owned()), "{bold:?}");
        assert!(bold.contains(&"zwölf Monate".to_owned()), "{bold:?}");
    }

    #[test]
    fn the_sentences_name_usage_growth_and_whether_it_fits() {
        let (first, second, high) = storage_lines(&stock(200, 150, 10, 10_240));
        assert!(
            first.starts_with("200,0 MB von 10,0 GB belegt (2,0 Prozent)"),
            "{first}"
        );
        assert!(first.contains("150,0 MB Messwerte"), "{first}");
        assert!(
            second.contains("15,0 MB pro Tag") && !second.contains("Voll"),
            "{second}"
        );
        assert!(!high);
        // 15 MB am Tag über zwölf Monate sind mehr als 5 GB.
        let (_, second, high) = storage_lines(&stock(200, 150, 10, 5_120));
        // 5 GB minus 200 MB bei 15 MB am Tag: gut zehn Monate.
        assert!(second.contains("Voll voraussichtlich in etwa 11 Monaten"), "{second}");
        assert!(high);
    }

    #[test]
    fn the_full_date_is_said_in_months() {
        assert_eq!(full_in(0.0), "schon jetzt");
        assert_eq!(full_in(0.4), "in weniger als einem Monat");
        assert_eq!(full_in(1.2), "in etwa einem Monat");
        assert_eq!(full_in(7.4), "in etwa 7 Monaten");
    }

    #[test]
    fn without_a_day_of_data_the_forecast_says_what_is_missing() {
        let (first, second, high) = storage_lines(&stock(10, 5, 0, 5_120));
        assert!(first.starts_with("10,0 MB von 5,0 GB"), "{first}");
        assert!(second.contains("fehlen noch Messwerte"), "{second}");
        assert!(!high);
        let (first, ..) = storage_lines(&DataStock::default());
        assert!(first.contains("belegt, davon"), "{first}");
    }
}
