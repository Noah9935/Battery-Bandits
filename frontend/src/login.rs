//! Anmeldung: Maske vor allen Daten, plus Benutzerfeld und Abmelden in der Kopfleiste.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::app::Logo;
use crate::icons::Icon;
use crate::state::Store;

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

/// Anfangsbuchstaben für den Kreis neben dem Namen, höchstens zwei.
pub fn initials(display_name: &str) -> String {
    let letters: String = display_name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() { "?".to_owned() } else { letters }
}

#[component]
pub fn LoginScreen() -> impl IntoView {
    let s = expect_context::<Store>();
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

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
            match api::login(&name, &secret).await {
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
            <form class="auth-card" on:submit=submit>
                <div class="auth-brand">
                    <Logo/>
                    <div>
                        <h1>"Battery Bandits"</h1>
                        <p class="sub">"Leitstand für TrafficNode-Batterien"</p>
                    </div>
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
                {move || {
                    error
                        .get()
                        .or_else(|| s.error.get())
                        .map(|text| view! { <p class="form-error" role="alert">{text}</p> })
                }}
                <button class="btn btn-primary btn-lg auth-submit" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "Einen Moment ..." } else { "Anmelden" }}
                </button>
                <p class="auth-hint">
                    "Nach fünf falschen Passwörtern ist der Benutzername eine Minute lang gesperrt."
                </p>
            </form>
        </div>
    }
}

/// Name der angemeldeten Person mit Abmelden, rechts in der Kopfleiste.
#[component]
pub fn UserChip() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            s.me
                .get()
                .map(|me| {
                    view! {
                        <div class="tb-user">
                            <span class="avatar" aria-hidden="true">{initials(&me.display_name)}</span>
                            <span class="tb-user-name" title=me.display_name.clone()>{me.display_name.clone()}</span>
                            <button
                                class="icon-btn"
                                aria-label="Abmelden"
                                title="Abmelden"
                                on:click=move |_| s.sign_out()
                            >
                                <Icon name="log-out"/>
                            </button>
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
    fn empty_inputs_are_named() {
        assert_eq!(check_inputs("", "x"), Err("Bitte den Benutzernamen eintragen."));
        assert_eq!(check_inputs("   ", "x"), Err("Bitte den Benutzernamen eintragen."));
        assert_eq!(check_inputs("noah", ""), Err("Bitte das Passwort eintragen."));
        assert_eq!(check_inputs("noah", "geheim"), Ok(()));
    }

    #[test]
    fn initials_use_the_first_two_words() {
        assert_eq!(initials("Noah Boufercha"), "NB");
        assert_eq!(initials("fynn"), "F");
        assert_eq!(initials("Anna Maria Schmidt"), "AM");
        assert_eq!(initials("  "), "?");
        assert_eq!(initials("ärger Öl"), "ÄÖ");
    }
}
