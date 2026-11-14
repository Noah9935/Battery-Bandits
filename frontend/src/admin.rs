//! Kontenverwaltung für Admins (in den Optionen): Konten anlegen, die Rolle vergeben, ein Passwort zurücksetzen oder
//! ein Konto löschen. Registrieren kann sich niemand selbst, Konten legt ein Admin an.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::{Account, AccountUpdate, NewAccount};

use crate::avatar::Avatar;
use crate::icons::Icon;
use crate::limit::MoreButton;
use crate::profile::role_text;
use crate::state::{Page, Store};
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
    let open = RwSignal::new(false);
    let username = account.username.clone();
    let admin = account.admin;
    // Das feste Konto admin behält immer alle Rechte: Es gibt keinen Knopf zum Entziehen oder Löschen.
    let permanent = account.username == shared::api::PERMANENT_ADMIN;
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
            <button
                class="account-main"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <Avatar id=account.avatar class="account-avatar"/>
                <span class="account-who">
                    <strong>{account.display_name.clone()}</strong>
                    <span class="muted account-line">{format!("{} · {} · seit {}", account.username, role_text(account.admin), jsutil::date_label(account.created_at))}</span>
                </span>
                <span class="account-col mono-num">{account.username.clone()}</span>
                <span class=if account.admin { "account-col account-role is-admin" } else { "account-col account-role" }>
                    {role_text(account.admin)}
                </span>
                <span class="account-col muted mono-num">{jsutil::date_label(account.created_at)}</span>
                <span class="account-chevron">
                    {move || view! { <Icon name=if open.get() { "chevron-up" } else { "chevron-down" }/> }}
                </span>
            </button>
            <div class=move || if open.get() { "account-actions" } else { "account-actions is-closed" }>
                {(!permanent)
                    .then(|| {
                        view! {
                            <button class="btn btn-sm" on:click=toggle_admin.clone()>
                                {if admin { "Admin entziehen" } else { "Zum Admin machen" }}
                            </button>
                        }
                    })}
                {permanent
                    .then(|| view! { <span class="muted">"Festes Konto: behält immer alle Rechte"</span> })}
                <button class="btn btn-sm" on:click=move |_| reset_open.update(|o| *o = !*o)>"Passwort setzen"</button>
                {(!own && !permanent)
                    .then(|| {
                        view! {
                            <button class="btn btn-sm btn-danger" on:click=delete>
                                {move || if confirm_delete.get() { "Wirklich löschen?" } else { "Löschen" }}
                            </button>
                        }
                    })}
            </div>
            <Show when=move || open.get() && reset_open.get()>
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

/// Wie viele Konten auf einmal sichtbar sind.
const ACCOUNT_STEP: usize = 8;

/// Passt das Konto zur Suche (Benutzername oder Anzeigename, ohne Beachtung der Schreibweise)?
pub fn matches_search(account: &Account, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || account.username.to_lowercase().contains(&query)
        || account.display_name.to_lowercase().contains(&query)
}

/// Kurzer Eintrag in den Optionen: Zahl der Konten und der Weg zur Verwaltung. Nur für Admins.
#[component]
pub fn AccountsEntry() -> impl IntoView {
    let s = expect_context::<Store>();
    let accounts = RwSignal::new(Vec::<Account>::new());
    spawn_local(async move {
        if let Ok(list) = api::admin_users().await {
            accounts.set(list);
        }
    });
    let summary = move || {
        let (all, admins) = accounts.with(|a| (a.len(), a.iter().filter(|a| a.admin).count()));
        match all {
            0 => "Konten anlegen, Rollen vergeben und Passwörter zurücksetzen.".to_owned(),
            1 => "1 Konto, davon 1 Admin.".to_owned(),
            n => format!(
                "{n} Konten, davon {admins} Admin{}.",
                if admins == 1 { "" } else { "s" }
            ),
        }
    };
    view! {
        <section class="surface surface-pad accounts-entry">
            <div>
                <h2>"Konten"</h2>
                <p class="muted">{summary}</p>
            </div>
            <button class="btn" on:click=move |_| s.go_page(Page::Accounts)>
                "Konten verwalten"
                <Icon name="chevron-right"/>
            </button>
        </section>
    }
}

/// Die Seite zur Kontenverwaltung: Suche, Liste in kleinen Schritten und das Formular zum Anlegen. Nur für Admins.
#[component]
pub fn AccountsPage() -> impl IntoView {
    let s = expect_context::<Store>();
    let accounts = RwSignal::new(Vec::<Account>::new());
    let shown = RwSignal::new(ACCOUNT_STEP);
    let search = RwSignal::new(String::new());
    let form_open = RwSignal::new(false);
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
    // Die Seite bleibt im Speicher: Beim Öffnen frisch laden.
    Effect::new(move |_| {
        if s.page.get() == Page::Accounts {
            load();
        }
    });
    let reload = Callback::new(move |()| load());
    let filtered = Memo::new(move |_| {
        let query = search.get();
        accounts.with(|list| {
            list.iter()
                .filter(|a| matches_search(a, &query))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let total = Signal::derive(move || filtered.with(Vec::len));
    Effect::new(move |_| {
        search.track();
        shown.set(ACCOUNT_STEP);
    });
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
                    form_open.set(false);
                    load();
                }
                Err(e) => problem.set(Some(e)),
            }
        });
    };
    let count_text = move || {
        let all = accounts.with(Vec::len);
        let found = total.get();
        if found == all {
            format!("{all} Konten")
        } else {
            format!("{found} von {all} Konten")
        }
    };
    view! {
        <div class="view">
            <header class="view-head">
                <button class="btn btn-ghost btn-sm back-link" on:click=move |_| s.go_page(Page::Options)>
                    <Icon name="arrow-left"/>
                    "Optionen"
                </button>
                <h1>"Konten"</h1>
                <p class="sub">
                    "Registrieren kann sich niemand selbst: Konten legt ein Admin an. Das Profilbild wird zufällig zugewiesen."
                </p>
            </header>
            <section class="surface surface-pad admin-users">
                <div class="accounts-tools">
                    <label class="field grow">
                        <span class="sr-only">"Konten suchen"</span>
                        <input
                            type="search"
                            placeholder="Name oder Benutzername suchen"
                            autocomplete="off"
                            prop:value=move || search.get()
                            on:input=move |ev| search.set(event_target_value(&ev))
                        />
                    </label>
                    <button class="btn btn-primary" on:click=move |_| form_open.update(|o| *o = !*o)>
                        {move || if form_open.get() { "Abbrechen" } else { "Konto anlegen" }}
                    </button>
                </div>
                <Show when=move || form_open.get()>
                    <form class="form account-form" on:submit=create>
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
                        <button class="btn btn-primary" type="submit">"Konto speichern"</button>
                    </form>
                </Show>
                <p class="muted accounts-count">{count_text}</p>
                <div class="account-head" aria-hidden="true">
                    <span></span>
                    <span>"Name"</span>
                    <span>"Benutzername"</span>
                    <span>"Rolle"</span>
                    <span>"Seit"</span>
                    <span></span>
                </div>
                <ul class="account-list">
                    {move || {
                        let list = filtered.get();
                        if list.is_empty() {
                            return view! { <li class="empty">"Kein Konto passt zur Suche."</li> }.into_any();
                        }
                        list.into_iter()
                            .take(shown.get())
                            .map(|account| view! { <AccountRow account=account reload=reload/> })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
                <MoreButton total=total shown=shown step=ACCOUNT_STEP/>
            </section>
        </div>
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

    #[test]
    fn the_search_finds_accounts_by_either_name() {
        let account = Account {
            username: "lena".into(),
            display_name: "Lena Muster".into(),
            admin: false,
            avatar: 3,
            created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
        };
        assert!(matches_search(&account, ""));
        assert!(matches_search(&account, "  LEN "));
        assert!(matches_search(&account, "muster"));
        assert!(!matches_search(&account, "fynn"));
    }
}
