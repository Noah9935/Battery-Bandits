//! Profil: Bild, Name und Rolle, Passwort ändern, abmelden. Von hier führt das Zahnrad zu den Optionen.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::avatar::Avatar;
use crate::icons::Icon;
use crate::state::{Page, Store};

/// Prüft die Eingaben des Formulars, bevor sie zum Server gehen.
pub fn password_form_error(current: &str, new: &str, repeat: &str) -> Option<&'static str> {
    if current.is_empty() {
        return Some("Bitte das aktuelle Passwort eingeben.");
    }
    if new != repeat {
        return Some("Die beiden neuen Passwörter sind nicht gleich.");
    }
    shared::api::check_new_password(current, new).err()
}

/// Rolle in Worten.
pub fn role_text(admin: bool) -> &'static str {
    if admin { "Admin" } else { "Mitarbeit" }
}

#[component]
pub fn ProfilePage() -> impl IntoView {
    let s = expect_context::<Store>();
    let current = RwSignal::new(String::new());
    let new = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        let (c, n, r) = (current.get_untracked(), new.get_untracked(), repeat.get_untracked());
        if let Some(message) = password_form_error(&c, &n, &r) {
            return problem.set(Some(message.to_owned()));
        }
        problem.set(None);
        busy.set(true);
        spawn_local(async move {
            match api::change_password(&c, &n).await {
                Ok(_) => {
                    current.set(String::new());
                    new.set(String::new());
                    repeat.set(String::new());
                    s.say("Passwort geändert. Auf anderen Geräten musst du dich neu anmelden.");
                }
                Err(e) => problem.set(Some(e)),
            }
            busy.set(false);
        });
    };
    view! {
        <div class="view">
            {move || {
                s.me
                    .get()
                    .map(|me| {
                        view! {
                            <header class="view-head profile-head">
                                <Avatar id=me.avatar class="profile-avatar"/>
                                <div class="profile-who">
                                    <h1>{me.display_name.clone()}</h1>
                                    <p class="sub">{format!("{} · {}", me.username, role_text(me.admin))}</p>
                                </div>
                                <button
                                    class="icon-btn gear"
                                    aria-label="Optionen"
                                    title="Optionen"
                                    on:click=move |_| s.go_page(Page::Options)
                                >
                                    <Icon name="settings"/>
                                </button>
                            </header>
                        }
                    })
            }}
            <section class="surface surface-pad">
                <h2>"Passwort ändern"</h2>
                <p class="muted">"Danach gilt das neue Passwort. Auf allen anderen Geräten wirst du abgemeldet."</p>
                <form class="form" on:submit=submit>
                    <label class="field">
                        <span>"Aktuelles Passwort"</span>
                        <input
                            type="password"
                            autocomplete="current-password"
                            prop:value=move || current.get()
                            on:input=move |ev| current.set(event_target_value(&ev))
                        />
                    </label>
                    <label class="field">
                        <span>"Neues Passwort (mindestens 8 Zeichen)"</span>
                        <input
                            type="password"
                            autocomplete="new-password"
                            prop:value=move || new.get()
                            on:input=move |ev| new.set(event_target_value(&ev))
                        />
                    </label>
                    <label class="field">
                        <span>"Neues Passwort wiederholen"</span>
                        <input
                            type="password"
                            autocomplete="new-password"
                            prop:value=move || repeat.get()
                            on:input=move |ev| repeat.set(event_target_value(&ev))
                        />
                    </label>
                    {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                    <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                        "Passwort ändern"
                    </button>
                </form>
            </section>
            <section class="surface surface-pad">
                <h2>"Anmeldung"</h2>
                <p class="muted">
                    "Mit \"Angemeldet bleiben\" erinnert sich dieses Gerät sehr lange an dich. Abmelden beendet das sofort."
                </p>
                <div class="btn-row">
                    <button class="btn" on:click=move |_| s.sign_out()>
                        <Icon name="log-out"/>
                        "Abmelden"
                    </button>
                </div>
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_checks_before_anything_is_sent() {
        assert_eq!(
            password_form_error("", "neues-passwort", "neues-passwort"),
            Some("Bitte das aktuelle Passwort eingeben.")
        );
        assert_eq!(
            password_form_error("alt-passwort", "neues-passwort", "anderes-passwort"),
            Some("Die beiden neuen Passwörter sind nicht gleich.")
        );
        assert!(password_form_error("alt-passwort", "kurz", "kurz").is_some());
        assert!(password_form_error("gleich-passwort", "gleich-passwort", "gleich-passwort").is_some());
        assert_eq!(
            password_form_error("alt-passwort", "neues-passwort", "neues-passwort"),
            None
        );
    }

    #[test]
    fn roles_have_words() {
        assert_eq!(role_text(true), "Admin");
        assert_eq!(role_text(false), "Mitarbeit");
    }
}
