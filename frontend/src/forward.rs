//! Weiterleitung im Postfach: Regel für automatische Meldungen und ein Tipp pro Eintrag.
//!
//! Zwei Wege: Die Regel lässt den Server neue Meldungen per E-Mail senden (braucht den Zugang zum Mailserver in der
//! `.env`, die Person trägt nur ihre Adresse ein). Der Tipp auf "Weiterleiten" öffnet das Mailprogramm mit
//! vorbereitetem Text und funktioniert immer, auch ohne Einrichtung auf dem Server.

use leptos::ev;
use leptos::html;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::notify::{NotificationRule, mailto_url};
use wasm_bindgen::JsCast;

use crate::api::{self, TestResult};
use crate::icons::Icon;
use crate::jsutil;
use crate::share::{Report, device_report};
use crate::state::Store;
use crate::{format, urgency};

/// Link, der einen Eintrag per E-Mail weiterleitet. Die Adresse stammt aus der Regel, falls dort eine steht, sonst
/// wählt man den Empfänger im Mailprogramm selbst. `base` ist die Adresse der Oberfläche.
pub fn item_mail_link(text: &str, device_id: Option<&str>, rule: Option<&NotificationRule>, base: &str) -> String {
    let message = format!("Battery Bandits: {text}");
    let subject: String = message.chars().take(80).collect();
    let body = match (device_id, base.trim().trim_end_matches('/')) {
        (Some(id), base) if !base.is_empty() => format!("{message} {base}/geraete/{id}"),
        _ => message,
    };
    let to = rule.map(|r| r.email_address.as_str()).filter(|t| !t.trim().is_empty());
    mailto_url(to, &subject, &body)
}

/// Schließt ein aufgeklapptes Menü, sobald irgendwo außerhalb von `root` getippt oder geklickt wird.
/// Ein Tipp im Menü selbst (und auf den Knopf, der es öffnet) zählt nicht, der Knopf schaltet wie bisher um.
pub fn close_on_outside_click(root: NodeRef<html::Div>, open: RwSignal<bool>) {
    let handle = window_event_listener(ev::pointerdown, move |event| {
        if !open.get_untracked() {
            return;
        }
        let Some(root) = root.get_untracked() else { return };
        let inside = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
            .is_some_and(|node| root.contains(Some(&node)));
        if !inside {
            open.set(false);
        }
    });
    on_cleanup(move || handle.remove());
}

/// Ergebnis der Probemail in einem Satz.
pub fn test_line(result: &TestResult) -> String {
    if result.ok {
        "Probemail gesendet.".to_owned()
    } else {
        result.detail.clone()
    }
}

/// Weiterleiten-Knopf eines Eintrags: öffnet das Mailprogramm mit vorbereitetem Text.
#[component]
pub fn ForwardMenu(text: String, device_id: Option<String>) -> impl IntoView {
    let s = expect_context::<Store>();
    let href = move || item_mail_link(&text, device_id.as_deref(), s.rule.get().as_ref(), &jsutil::origin());
    view! {
        <div class="forward">
            <a class="btn btn-sm" href=href aria-label="Per E-Mail weiterleiten" title="Per E-Mail weiterleiten">
                <Icon name="mail"/>
            </a>
        </div>
    }
}

/// Teilen-Knopf eines Geräts: schickt die Statistik als aufgereihten Text an jemand anderen. Auf dem Handy öffnet
/// sich das Teilen-Fenster des Telefons, sonst gibt es E-Mail und Kopieren. Die Messwerte als CSV bleiben als kleiner
/// Link erhalten.
#[component]
pub fn DeviceShare(id: String, #[prop(into)] csv_url: Signal<String>) -> impl IntoView {
    let s = expect_context::<Store>();
    let open = RwSignal::new(false);
    let copied = RwSignal::new(false);
    // Der Text entsteht beim Öffnen aus den aktuellen Werten, nicht beim Aufbau der Seite.
    let content = RwSignal::new(None::<(String, String, String)>);
    let wrap = NodeRef::<html::Div>::new();
    close_on_outside_click(wrap, open);
    let build = {
        let id = id.clone();
        move || {
            let device = s.device(&id)?;
            let critical = s
                .settings
                .with_untracked(|st| st.as_ref().map_or(20.0, |st| st.threshold_critical_pct));
            let alert = s.alerts.with_untracked(|a| {
                a.iter()
                    .find(|a| a.device_id == id && a.status == shared::AlertStatus::Open)
                    .cloned()
            });
            let now = jsutil::now_ms();
            let text = device_report(&Report {
                device: &device,
                critical_pct: critical,
                state_since: urgency::state_since(&device, alert.as_ref())
                    .map(|t| format::span((now - t.timestamp_millis() as f64) as i64)),
                last_contact: device.last_seen_at.map(jsutil::date_time_label),
                installed: device.battery_installed_at.map(jsutil::date_label),
                taken_at: chrono::DateTime::from_timestamp_millis(now as i64)
                    .map_or_else(String::new, jsutil::date_time_label),
            });
            let link = format!("{}/geraete/{}", jsutil::origin().trim_end_matches('/'), id);
            let title = format!("Battery Bandits: {}", format::device_title(&id, device.name.as_deref()));
            Some((title, text, link))
        }
    };
    let toggle = move |_| {
        if !open.get_untracked() {
            copied.set(false);
            content.set(build());
        }
        open.update(|o| *o = !*o);
    };
    view! {
        <div class="share-wrap" node_ref=wrap>
            <button class="btn" aria-expanded=move || open.get().to_string() on:click=toggle>
                <Icon name="share"/>
                "Teilen"
            </button>
            <Show when=move || open.get() && content.with(Option::is_some)>
                <div class="share-panel surface" data-noswipe="">
                    {move || {
                        content
                            .get()
                            .map(|(title, text, link)| {
                                let with_link = format!("{text}\n\n{link}");
                                let mail_href = mailto_url(None, &title, &with_link);
                                let (sys_title, sys_text, sys_link) = (title.clone(), text.clone(), link.clone());
                                let copy_text = with_link.clone();
                                view! {
                                    <pre class="share-preview">
                                        {text
                                            .lines()
                                            .map(|line| {
                                                let h = crate::share::highlight(line);
                                                view! {
                                                    {h.head}
                                                    <strong>{h.strong}</strong>
                                                    "\n"
                                                }
                                            })
                                            .collect_view()}
                                    </pre>
                                    <div class="share-actions">
                                        {jsutil::can_share()
                                            .then(|| {
                                                view! {
                                                    <button
                                                        class="btn btn-primary"
                                                        on:click=move |_| jsutil::system_share(&sys_title, &sys_text, &sys_link)
                                                    >
                                                        <Icon name="share"/>
                                                        "Senden an ..."
                                                    </button>
                                                }
                                            })}
                                        <a class="btn" href=mail_href>
                                            <Icon name="mail"/>
                                            "E-Mail"
                                        </a>
                                        <button
                                            class="btn"
                                            on:click=move |_| {
                                                if jsutil::copy_text(&copy_text) {
                                                    copied.set(true);
                                                } else {
                                                    s.say("Kopieren ist hier nicht erlaubt. Den Text markieren und von Hand kopieren.");
                                                }
                                            }
                                        >
                                            <Icon name="check"/>
                                            {move || if copied.get() { "Kopiert" } else { "Text kopieren" }}
                                        </button>
                                    </div>
                                    <a class="link share-csv" href=move || csv_url.get() download="">
                                        "Messwerte als CSV laden"
                                    </a>
                                }
                            })
                    }}
                </div>
            </Show>
        </div>
    }
}

/// Regel für die automatische Weiterleitung: "Neue Meldungen automatisch senden an ...".
#[component]
pub fn ForwardCard() -> impl IntoView {
    let s = expect_context::<Store>();
    let draft = RwSignal::new(NotificationRule {
        only_critical: true,
        ..NotificationRule::default()
    });
    let loaded = StoredValue::new(false);
    let problem = RwSignal::new(None::<String>);
    let result = RwSignal::new(None::<TestResult>);
    let busy = RwSignal::new(false);

    // Die gespeicherte Regel einmal in den Entwurf übernehmen, sobald sie da ist.
    Effect::new(move |_| {
        if let Some(rule) = s.rule.get()
            && !loaded.get_value()
        {
            loaded.set_value(true);
            draft.set(rule);
        }
    });

    let save = move |_| {
        let rule = draft.get_untracked();
        if let Err(message) = rule.validate() {
            return problem.set(Some(message));
        }
        busy.set(true);
        spawn_local(async move {
            match api::save_notification_rule(&rule).await {
                Ok(saved) => {
                    s.rule.set(Some(saved.clone()));
                    draft.set(saved);
                    problem.set(None);
                    s.say("Weiterleitung gespeichert.");
                }
                Err(e) => problem.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let test = move |_| {
        busy.set(true);
        result.set(None);
        spawn_local(async move {
            match api::test_notification().await {
                Ok(sent) => {
                    problem.set(None);
                    result.set(Some(sent));
                }
                Err(e) => problem.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="surface forward-card">
            <div class="surface-head">
                <h2>"Per E-Mail weiterleiten"</h2>
            </div>
            <div class="surface-body">
                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=move || draft.with(|r| r.enabled)
                        on:change=move |ev| draft.update(|r| r.enabled = event_target_checked(&ev))
                    />
                    <span>"Neue Meldungen automatisch senden"</span>
                </label>
                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=move || draft.with(|r| r.only_critical)
                        on:change=move |ev| draft.update(|r| r.only_critical = event_target_checked(&ev))
                    />
                    <span>"Nur kritische Meldungen (leer, offline, ausgefallen)"</span>
                </label>
                <label class="field">
                    <span>"E-Mail-Adresse"</span>
                    <input
                        type="text"
                        inputmode="email"
                        placeholder="name@beispiel.de"
                        prop:value=move || draft.with(|r| r.email_address.clone())
                        on:input=move |ev| draft.update(|r| r.email_address = event_target_value(&ev))
                    />
                </label>
                {move || {
                    (!s.mail.get().ready)
                        .then(|| view! { <p class="muted forward-hint">"Auf dem Server nicht eingerichtet: Senden geht nur per Tipp auf Weiterleiten."</p> })
                }}
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                {move || {
                    result
                        .get()
                        .map(|r| view! { <p class=if r.ok { "forward-result ok" } else { "forward-result fail" }>{test_line(&r)}</p> })
                }}
                <div class="btn-row">
                    <button class="btn btn-primary" disabled=move || busy.get() on:click=save>"Speichern"</button>
                    <button class="btn" disabled=move || busy.get() on:click=test>"Probemail senden"</button>
                </div>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> NotificationRule {
        NotificationRule {
            email_address: "noah@beispiel.de".into(),
            ..NotificationRule::default()
        }
    }

    #[test]
    fn an_entry_gets_a_mail_link_with_the_text_and_the_device_link() {
        let link = item_mail_link(
            "TN0001: Ladestand kritisch",
            Some("TN0001"),
            None,
            "https://bandits.example.org/",
        );
        assert!(
            link.starts_with("mailto:?subject=Battery%20Bandits%3A%20TN0001"),
            "ohne Regel wählt man den Empfänger selbst: {link}"
        );
        assert!(
            link.contains("geraete%2FTN0001") || link.contains("geraete/TN0001"),
            "{link}"
        );
    }

    #[test]
    fn the_address_of_the_rule_is_filled_in() {
        let link = item_mail_link("Text", None, Some(&rule()), "");
        assert!(link.starts_with("mailto:noah@beispiel.de?"), "{link}");
        assert!(!link.contains("geraete"), "ohne Gerät und Adresse kein Link");
        let blank = NotificationRule {
            email_address: "  ".into(),
            ..rule()
        };
        assert!(item_mail_link("Text", None, Some(&blank), "").starts_with("mailto:?"));
    }

    #[test]
    fn a_long_text_is_cut_for_the_mail_subject_only() {
        let text = "x".repeat(200);
        let link = item_mail_link(&text, None, None, "");
        let subject = link.split("subject=").nth(1).unwrap().split('&').next().unwrap();
        assert!(subject.len() < 120, "{subject}");
        assert!(link.len() > 200, "der Text selbst bleibt vollständig");
    }

    #[test]
    fn a_test_result_is_one_readable_line() {
        let ok = TestResult {
            ok: true,
            detail: "gesendet".into(),
        };
        let failed = TestResult {
            ok: false,
            detail: "E-Mail ist auf dem Server nicht eingerichtet.".into(),
        };
        assert_eq!(test_line(&ok), "Probemail gesendet.");
        assert_eq!(test_line(&failed), "E-Mail ist auf dem Server nicht eingerichtet.");
    }
}
