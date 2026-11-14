//! Anmeldung: Maske vor allen Daten, plus Benutzerfeld und Abmelden in der Kopfleiste.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::app::Logo;
use crate::avatar::Avatar;
use crate::state::{Page, Store};

/// Prüft die Eingaben, bevor sie an den Server gehen. `Err` ist ein Hinweistext für die Maske.
pub fn check_inputs(username: &str, password: &str) -> Result<(), &'static str> {
    if username.trim().is_empty() {
        return Err("Bitte den Benutzernamen eintragen.");
    }
    if password.is_empty() {
        return Err("Bitte das Passwort eintragen.");
    }
    Ok(())
}

#[component]
pub fn LoginScreen() -> impl IntoView {
    let s = expect_context::<Store>();
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    // Auf dem eigenen Gerät angemeldet bleiben ist die Regel: Das Token ist sehr langlebig.
    let remember = RwSignal::new(true);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        let (name, secret) = (username.get_untracked(), password.get_untracked());
        if let Err(hint) = check_inputs(&name, &secret) {
            error.set(Some(hint.to_owned()));
            return;
        }
        error.set(None);
        busy.set(true);
        spawn_local(async move {
            match api::login(&name, &secret, remember.get_untracked()).await {
                Ok(me) => {
                    password.set(String::new());
                    s.sign_in(me);
                }
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="auth-page">
            <section class="auth-brand" aria-hidden="true">
                <div class="auth-mark">
                    <Logo/>
                    <span class="wordmark">"Battery Bandits"</span>
                </div>
            </section>
            <form class="auth-card" on:submit=submit>
                <div class="auth-head">
                    <h1>"Anmelden"</h1>
                    <p class="sub">"Mit deinem Konto für Battery Bandits."</p>
                </div>
                <label class="field">
                    <span>"Benutzername"</span>
                    <input
                        type="text"
                        autocomplete="username"
                        autocapitalize="none"
                        spellcheck="false"
                        autofocus
                        prop:value=move || username.get()
                        on:input=move |ev| username.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Passwort"</span>
                    <input
                        type="password"
                        autocomplete="current-password"
                        prop:value=move || password.get()
                        on:input=move |ev| password.set(event_target_value(&ev))
                    />
                </label>
                <label class="check auth-remember">
                    <input
                        type="checkbox"
                        prop:checked=move || remember.get()
                        on:change=move |ev| remember.set(event_target_checked(&ev))
                    />
                    <span>"Angemeldet bleiben"</span>
                </label>
                {move || {
                    error
                        .get()
                        .or_else(|| s.error.get())
                        .map(|text| view! { <p class="form-error" role="alert">{text}</p> })
                }}
                <button class="btn btn-primary btn-lg auth-submit" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "Einen Moment ..." } else { "Anmelden" }}
                </button>
            </form>
        </div>
    }
}

/// Profilbild oben rechts: Ein Tipp öffnet das Profil, von dort geht es zu den Optionen und zum Abmelden.
#[component]
pub fn UserChip() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            s.me
                .get()
                .map(|me| {
                    view! {
                        <button
                            class=move || if s.page.get() == Page::Profile && s.device_page.with(Option::is_none) { "tb-user is-active" } else { "tb-user" }
                            aria-label="Profil und Optionen"
                            title=me.display_name.clone()
                            on:click=move |_| s.go_page(Page::Profile)
                        >
                            <Avatar id=me.avatar class="tb-avatar"/>
                        </button>
                    }
                })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inputs_are_named() {
        assert_eq!(check_inputs("", "x"), Err("Bitte den Benutzernamen eintragen."));
        assert_eq!(check_inputs("   ", "x"), Err("Bitte den Benutzernamen eintragen."));
        assert_eq!(check_inputs("noah", ""), Err("Bitte das Passwort eintragen."));
        assert_eq!(check_inputs("noah", "geheim"), Ok(()));
    }
}
