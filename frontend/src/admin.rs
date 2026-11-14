//! Kontenverwaltung für Admins (in den Optionen): Konten anlegen, die Rolle vergeben, ein Passwort zurücksetzen oder
//! ein Konto löschen. Registrieren kann sich niemand selbst, Konten legt ein Admin an.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::{Account, AccountUpdate, NewAccount};

use crate::avatar::Avatar;
use crate::limit::{MoreButton, STEP};
use crate::profile::role_text;
use crate::state::Store;
use crate::{api, jsutil};

/// Baut das neue Konto aus den Eingaben und prüft sie wie der Server.
pub fn build_account(
    username: &str,
    display_name: &str,
    password: &str,
    admin: bool,
) -> Result<NewAccount, &'static str> {
    let username = username.trim().to_lowercase();
    shared::api::check_username(&username)?;
    shared::api::check_display_name(display_name)?;
    shared::api::check_new_password("", password)?;
    Ok(NewAccount {
        username,
        display_name: display_name.trim().to_owned(),
        password: password.to_owned(),
        admin,
    })
}

#[component]
fn AccountRow(account: Account, reload: Callback<()>) -> impl IntoView {
    let s = expect_context::<Store>();
    let me = s.me.with_untracked(|m| m.as_ref().map(|m| m.username.clone()));
    let own = me.as_deref() == Some(account.username.as_str());
    let reset_open = RwSignal::new(false);
    let reset = RwSignal::new(String::new());
    let confirm_delete = RwSignal::new(false);
    let username = account.username.clone();
    let admin = account.admin;
    let toggle_admin = {
        let username = username.clone();
        move |_| {
            let username = username.clone();
            spawn_local(async move {
                let update = AccountUpdate {
                    admin: Some(!admin),
                    ..AccountUpdate::default()
                };
                match api::admin_update(&username, &update).await {
                    Ok(_) => {
                        s.say(if admin {
                            "Admin-Rolle entzogen."
                        } else {
                            "Zum Admin gemacht."
                        });
                        reload.run(());
                        // Betrifft es die eigene Rolle, zeigt die Oberfläche es nach dem Neuladen von `me`.
                        s.refresh_me();
                    }
                    Err(e) => s.say(e),
                }
            });
        }
    };
    let set_password = {
        let username = username.clone();
        move |_| {
            let password = reset.get_untracked();
            if let Err(message) = shared::api::check_new_password("", &password) {
                return s.say(message);
            }
            let username = username.clone();
            spawn_local(async move {
                let update = AccountUpdate {
                    password: Some(password),
                    ..AccountUpdate::default()
                };
                match api::admin_update(&username, &update).await {
                    Ok(_) => {
                        s.say("Passwort gesetzt. Die Person muss sich neu anmelden.");
                        reset.set(String::new());
                        reset_open.set(false);
                    }
                    Err(e) => s.say(e),
                }
            });
        }
    };
    let delete = {
        let username = username.clone();
        move |_| {
            if !confirm_delete.get_untracked() {
                return confirm_delete.set(true);
            }
            let username = username.clone();
            spawn_local(async move {
                match api::admin_delete(&username).await {
                    Ok(_) => {
                        s.say("Konto gelöscht.");
                        reload.run(());
                    }
                    Err(e) => s.say(e),
                }
            });
        }
    };
    view! {
        <li class="account-row">
            <div class="account-main">
                <Avatar id=account.avatar class="account-avatar"/>
                <div class="account-who">
                    <strong>{account.display_name.clone()}</strong>
                    <span class="muted">{format!("{} · {} · seit {}", account.username, role_text(account.admin), jsutil::date_label(account.created_at))}</span>
                </div>
            </div>
            <div class="account-actions">
                <button class="btn btn-sm" on:click=toggle_admin>
                    {if admin { "Admin entziehen" } else { "Zum Admin machen" }}
                </button>
                <button class="btn btn-sm" on:click=move |_| reset_open.update(|o| *o = !*o)>"Passwort setzen"</button>
                {(!own)
                    .then(|| {
                        view! {
                            <button class="btn btn-sm btn-danger" on:click=delete>
                                {move || if confirm_delete.get() { "Wirklich löschen?" } else { "Löschen" }}
                            </button>
                        }
                    })}
            </div>
            <Show when=move || reset_open.get()>
                <div class="account-reset">
                    <input
                        type="text"
                        placeholder="Neues Passwort, mindestens 8 Zeichen"
                        autocomplete="off"
                        prop:value=move || reset.get()
                        on:input=move |ev| reset.set(event_target_value(&ev))
                    />
                    <button class="btn btn-sm btn-primary" on:click=set_password.clone()>"Setzen"</button>
                </div>
            </Show>
        </li>
    }
}

/// Liste der Konten und das Formular zum Anlegen. Nur für Admins.
#[component]
pub fn AdminUsers() -> impl IntoView {
    let s = expect_context::<Store>();
    let accounts = RwSignal::new(Vec::<Account>::new());
    let shown = RwSignal::new(STEP);
    let total = Signal::derive(move || accounts.with(Vec::len));
    let username = RwSignal::new(String::new());
    let display_name = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let admin = RwSignal::new(false);
    let problem = RwSignal::new(None::<String>);
    let load = move || {
        spawn_local(async move {
            if let Ok(list) = api::admin_users().await {
                accounts.set(list);
            }
        });
    };
    load();
    let reload = Callback::new(move |()| load());
    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let new = match build_account(
            &username.get_untracked(),
            &display_name.get_untracked(),
            &password.get_untracked(),
            admin.get_untracked(),
        ) {
            Ok(new) => new,
            Err(message) => return problem.set(Some(message.to_owned())),
        };
        problem.set(None);
        spawn_local(async move {
            match api::admin_create(&new).await {
                Ok(_) => {
                    s.say(format!("Konto {} angelegt.", new.username));
                    username.set(String::new());
                    display_name.set(String::new());
                    password.set(String::new());
                    admin.set(false);
                    load();
                }
                Err(e) => problem.set(Some(e)),
            }
        });
    };
    view! {
        <section class="surface surface-pad admin-users">
            <h2>"Konten"</h2>
            <p class="muted">
                "Registrieren kann sich niemand selbst: Konten legt ein Admin an. Das Profilbild wird zufällig zugewiesen."
            </p>
            <form class="form" on:submit=create>
                <label class="field">
                    <span>"Benutzername"</span>
                    <input
                        type="text"
                        autocapitalize="none"
                        spellcheck="false"
                        autocomplete="off"
                        prop:value=move || username.get()
                        on:input=move |ev| username.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Anzeigename"</span>
                    <input
                        type="text"
                        autocomplete="off"
                        prop:value=move || display_name.get()
                        on:input=move |ev| display_name.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Startpasswort (mindestens 8 Zeichen)"</span>
                    <input
                        type="text"
                        autocomplete="off"
                        prop:value=move || password.get()
                        on:input=move |ev| password.set(event_target_value(&ev))
                    />
                </label>
                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=move || admin.get()
                        on:change=move |ev| admin.set(event_target_checked(&ev))
                    />
                    <span>"Darf Konten verwalten (Admin)"</span>
                </label>
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                <button class="btn btn-primary" type="submit">"Konto anlegen"</button>
            </form>
            <ul class="account-list">
                {move || {
                    accounts
                        .get()
                        .into_iter()
                        .take(shown.get())
                        .map(|account| view! { <AccountRow account=account reload=reload/> })
                        .collect_view()
                }}
            </ul>
            <MoreButton total=total shown=shown/>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_account_is_checked_like_on_the_server() {
        let new = build_account("  Lena ", " Lena Muster ", "lena-passwort", true).unwrap();
        assert_eq!(
            (new.username.as_str(), new.display_name.as_str(), new.admin),
            ("lena", "Lena Muster", true)
        );
        assert!(build_account("x", "X", "lang-genug-1", false).is_err());
        assert!(build_account("mit leerzeichen", "X", "lang-genug-1", false).is_err());
        assert!(build_account("anna", "   ", "lang-genug-1", false).is_err());
        assert!(build_account("anna", "Anna", "kurz", false).is_err());
    }
}
