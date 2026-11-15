//! Weiterleitung im Postfach: Regel für automatische Meldungen und ein Tipp pro Eintrag.
//!
//! Zwei Wege: Die Regel lässt den Server neue Meldungen per E-Mail senden (braucht den Zugang zum Mailserver in der
//! `.env`, die Person trägt nur ihre Adresse ein). Der Tipp auf "Weiterleiten" öffnet WhatsApp oder das Mailprogramm
//! mit vorbereitetem Text und funktioniert immer, auch ohne Einrichtung auf dem Server.

use leptos::ev;
use leptos::html;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::notify::{Channel, NotificationRule, share_url};
use wasm_bindgen::JsCast;

use crate::api::{self, TestResult};
use crate::icons::Icon;
use crate::jsutil;
use crate::share::{Report, device_report};
use crate::state::Store;
use crate::{format, urgency};

/// Symbol eines Dienstes.
pub fn channel_icon(channel: Channel) -> &'static str {
    match channel {
        Channel::WhatsApp => "message-circle",
        Channel::Telegram => "send",
        Channel::Sms => "smartphone",
        Channel::Email => "mail",
    }
}

/// Links, die einen Eintrag über die vier Dienste weiterleiten. Das Ziel stammt aus der Regel, falls dort eines
/// steht, sonst wählt man den Empfänger im Dienst selbst. `base` ist die Adresse der Oberfläche.
pub fn item_links(
    text: &str,
    device_id: Option<&str>,
    rule: Option<&NotificationRule>,
    base: &str,
) -> Vec<(Channel, String)> {
    let message = format!("Battery Bandits: {text}");
    let subject: String = message.chars().take(80).collect();
    let link = match (device_id, base.trim().trim_end_matches('/')) {
        (Some(id), base) if !base.is_empty() => Some(format!("{base}/geraete/{id}")),
        _ => None,
    };
    Channel::OFFERED
        .into_iter()
        .map(|channel| {
            let target = rule.map(|r| r.target(channel)).filter(|t| !t.trim().is_empty());
            (channel, share_url(channel, target, &subject, &message, link.as_deref()))
        })
        .collect()
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

/// Ergebnis einer Probenachricht in einem Satz.
pub fn test_line(result: &TestResult) -> String {
    format!(
        "{}: {}",
        result.channel.label(),
        if result.ok {
            "gesendet".to_owned()
        } else {
            result.detail.clone()
        }
    )
}

/// Weiterleiten-Menü eines Eintrags: vier Links, jeder öffnet den Dienst mit vorbereitetem Text.
#[component]
pub fn ForwardMenu(text: String, device_id: Option<String>) -> impl IntoView {
    let s = expect_context::<Store>();
    let open = RwSignal::new(false);
    let wrap = NodeRef::<html::Div>::new();
    close_on_outside_click(wrap, open);
    view! {
        <div class="forward" node_ref=wrap>
            <button
                class="btn btn-sm btn-ghost"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <Icon name="share"/>
                "Weiterleiten"
            </button>
            <Show when=move || open.get()>
                <div class="forward-links" data-noswipe="">
                    {
                        let links = item_links(&text, device_id.as_deref(), s.rule.get_untracked().as_ref(), &jsutil::origin());
                        links
                            .into_iter()
                            .map(|(channel, href)| {
                                view! {
                                    <a class="btn btn-sm" href=href target="_blank" rel="noopener">
                                        <Icon name=channel_icon(channel)/>
                                        {channel.label()}
                                    </a>
                                }
                            })
                            .collect_view()
                    }
                </div>
            </Show>
        </div>
    }
}

/// Teilen-Knopf eines Geräts: schickt die Statistik als aufgereihten Text an jemand anderen. Auf dem Handy öffnet
/// sich das Teilen-Fenster des Telefons, sonst gibt es WhatsApp, Telegram, SMS, E-Mail und Kopieren. Die Messwerte
/// als CSV bleiben als kleiner Link erhalten.
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
                                let links: Vec<(Channel, String)> = Channel::OFFERED
                                    .into_iter()
                                    .map(|channel| {
                                        let href = if channel == Channel::Telegram {
                                            share_url(channel, None, &title, &text, Some(&link))
                                        } else if channel == Channel::WhatsApp {
                                            // WhatsApp macht aus *Text* dicke Schrift.
                                            share_url(channel, None, &title, &crate::share::whatsapp_markup(&with_link), None)
                                        } else {
                                            share_url(channel, None, &title, &with_link, None)
                                        };
                                        (channel, href)
                                    })
                                    .collect();
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
                                        {links
                                            .into_iter()
                                            .map(|(channel, href)| {
                                                view! {
                                                    <a class="btn" href=href target="_blank" rel="noopener">
                                                        <Icon name=channel_icon(channel)/>
                                                        {channel.label()}
                                                    </a>
                                                }
                                            })
                                            .collect_view()}
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

/// Regel für die automatische Weiterleitung: "Kritische Meldungen automatisch senden an ...".
#[component]
pub fn ForwardCard() -> impl IntoView {
    let s = expect_context::<Store>();
    let draft = RwSignal::new(NotificationRule {
        only_critical: true,
        ..NotificationRule::default()
    });
    let loaded = StoredValue::new(false);
    let problem = RwSignal::new(None::<String>);
    let results = RwSignal::new(Vec::<TestResult>::new());
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
        let mut rule = draft.get_untracked();
        // Was die Oberfläche fürs automatische Senden nicht mehr anbietet, bleibt aus, auch wenn eine ältere Regel es
        // noch eingeschaltet hatte.
        for channel in Channel::ALL {
            if !Channel::AUTOMATIC.contains(&channel) {
                rule.set_on(channel, false);
            }
        }
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
        results.set(Vec::new());
        spawn_local(async move {
            match api::test_notifications().await {
                Ok(list) if list.is_empty() => {
                    problem.set(Some(
                        "Zum Testen einen Dienst einschalten und die Regel speichern.".to_owned(),
                    ));
                }
                Ok(list) => {
                    problem.set(None);
                    results.set(list);
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
                <ul class="forward-channels">
                    {Channel::AUTOMATIC
                        .into_iter()
                        .map(|channel| {
                            view! {
                                <li class="forward-channel">
                                    <label class="check">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || draft.with(|r| r.is_on(channel))
                                            on:change=move |ev| draft.update(|r| r.set_on(channel, event_target_checked(&ev)))
                                        />
                                        <span class="forward-name">
                                            <Icon name=channel_icon(channel)/>
                                            {channel.label()}
                                        </span>
                                    </label>
                                    <input
                                        type="text"
                                        inputmode="email"
                                        aria-label=channel.target_label()
                                        placeholder=channel.target_placeholder()
                                        prop:value=move || draft.with(|r| r.target(channel).to_owned())
                                        on:input=move |ev| draft.update(|r| r.set_target(channel, &event_target_value(&ev)))
                                    />
                                    {move || {
                                        (!s.channels.get().ready(channel))
                                            .then(|| view! { <p class="muted forward-hint">"Auf dem Server nicht eingerichtet: Senden geht nur per Tipp auf Weiterleiten."</p> })
                                    }}
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                <ul class="forward-results">
                    {move || {
                        results
                            .get()
                            .into_iter()
                            .map(|r| view! { <li class=if r.ok { "ok" } else { "fail" }>{test_line(&r)}</li> })
                            .collect_view()
                    }}
                </ul>
                <div class="btn-row">
                    <button class="btn btn-primary" disabled=move || busy.get() on:click=save>"Speichern"</button>
                    <button class="btn" disabled=move || busy.get() on:click=test>"Probenachricht senden"</button>
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
            telegram_chat_id: "123456789".into(),
            whatsapp_number: "0170 1234567".into(),
            email_address: "noah@beispiel.de".into(),
            ..NotificationRule::default()
        }
    }

    #[test]
    fn automatic_forwarding_offers_only_email_while_the_tap_keeps_both() {
        assert_eq!(Channel::AUTOMATIC, [Channel::Email]);
        assert!(
            Channel::OFFERED.contains(&Channel::WhatsApp),
            "der Tipp auf Weiterleiten braucht keine Einrichtung"
        );
    }

    #[test]
    fn every_channel_has_a_known_icon() {
        for channel in Channel::ALL {
            assert!(crate::icons::svg(channel_icon(channel)).is_some(), "{channel:?}");
        }
    }

    #[test]
    fn an_entry_gets_one_link_per_service_with_the_text_and_the_device_link() {
        let links = item_links(
            "TN0001: Ladestand kritisch",
            Some("TN0001"),
            None,
            "https://bandits.example.org/",
        );
        // Angeboten werden nur WhatsApp und E-Mail.
        assert_eq!(
            links.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            [Channel::WhatsApp, Channel::Email]
        );
        let by = |c: Channel| &links.iter().find(|(ch, _)| *ch == c).unwrap().1;
        assert!(by(Channel::WhatsApp).starts_with("https://wa.me/?text=Battery%20Bandits%3A%20TN0001"));
        assert!(by(Channel::WhatsApp).contains("geraete%2FTN0001") || by(Channel::WhatsApp).contains("geraete/TN0001"));
        assert!(by(Channel::Email).starts_with("mailto:?subject=Battery%20Bandits%3A%20TN0001"));
    }

    #[test]
    fn the_targets_of_the_rule_are_filled_in() {
        let links = item_links("Text", None, Some(&rule()), "");
        let by = |c: Channel| &links.iter().find(|(ch, _)| *ch == c).unwrap().1;
        assert!(by(Channel::WhatsApp).starts_with("https://wa.me/491701234567?text="));
        assert!(by(Channel::Email).starts_with("mailto:noah@beispiel.de?"));
        assert!(
            !by(Channel::WhatsApp).contains("geraete"),
            "ohne Gerät und Adresse kein Link"
        );
    }

    #[test]
    fn a_long_text_is_cut_for_the_mail_subject_only() {
        let text = "x".repeat(200);
        let links = item_links(&text, None, None, "");
        let mail = &links.iter().find(|(c, _)| *c == Channel::Email).unwrap().1;
        let subject = mail.split("subject=").nth(1).unwrap().split('&').next().unwrap();
        assert!(subject.len() < 120, "{subject}");
        let whatsapp = &links.iter().find(|(c, _)| *c == Channel::WhatsApp).unwrap().1;
        assert!(whatsapp.len() > 200, "der Text selbst bleibt vollständig");
    }

    #[test]
    fn a_test_result_is_one_readable_line() {
        let ok = TestResult {
            channel: Channel::Telegram,
            ok: true,
            detail: "gesendet".into(),
        };
        let failed = TestResult {
            channel: Channel::Sms,
            ok: false,
            detail: "SMS ist auf dem Server nicht eingerichtet.".into(),
        };
        assert_eq!(test_line(&ok), "Telegram: gesendet");
        assert_eq!(test_line(&failed), "SMS: SMS ist auf dem Server nicht eingerichtet.");
    }
}
