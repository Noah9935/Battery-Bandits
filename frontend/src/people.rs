//! Personen auswählen: ein Suchfeld, das zugleich ein Auswahlmenü ist. Mit vielen Konten ist eine Reihe von Knöpfen
//! zu lang, hier tippt man ein Stück vom Namen und tippt auf den Treffer.

use std::collections::BTreeSet;

use leptos::prelude::*;

use crate::state::Store;
use crate::swap::{short_names, visible_people};

/// Wie viele Treffer das Menü höchstens zeigt.
pub const HITS: usize = 8;

/// Die Treffer für die Eingabe ohne die schon gewählten und höchstens [`HITS`], dazu wie viele weitere es gibt.
pub fn pick_hits(
    people: &[(String, String)],
    users: &[shared::api::Me],
    query: &str,
    exclude: &BTreeSet<String>,
) -> (Vec<(String, String)>, usize) {
    let all: Vec<(String, String)> = visible_people(people, users, query, "")
        .into_iter()
        .filter(|(username, _)| !exclude.contains(username))
        .collect();
    let more = all.len().saturating_sub(HITS);
    (all.into_iter().take(HITS).collect(), more)
}

/// Suchfeld mit Auswahlmenü. `label` steht im Feld, solange man nicht sucht (zum Beispiel der gewählte Name).
/// `exclude` blendet schon gewählte Personen im Menü aus. Ein Tipp auf einen Treffer ruft `on_pick` mit dem
/// Benutzernamen auf, Enter nimmt den ersten Treffer.
#[component]
pub fn PersonSearch(
    #[prop(into)] on_pick: Callback<String>,
    #[prop(into)] label: Signal<String>,
    #[prop(into)] exclude: Signal<BTreeSet<String>>,
    #[prop(into)] placeholder: String,
) -> impl IntoView {
    let s = expect_context::<Store>();
    let query = RwSignal::new(String::new());
    let open = RwSignal::new(false);
    let people = Memo::new(move |_| s.users.with(|users| short_names(users)));
    let hits = Memo::new(move |_| {
        let q = query.get();
        let ex = exclude.get();
        people.with(|p| s.users.with(|users| pick_hits(p, users, &q, &ex)))
    });
    let pick = move |username: String| {
        on_pick.run(username);
        query.set(String::new());
        open.set(false);
    };
    view! {
        <div class="person-search">
            <input
                type="search"
                role="combobox"
                autocomplete="off"
                placeholder=placeholder
                aria-expanded=move || open.get().to_string()
                aria-label="Person suchen oder auswählen"
                prop:value=move || if open.get() { query.get() } else { label.get() }
                on:focus=move |_| {
                    query.set(String::new());
                    open.set(true);
                }
                on:blur=move |_| open.set(false)
                on:input=move |ev| {
                    query.set(event_target_value(&ev));
                    open.set(true);
                }
                on:keydown=move |ev| {
                    match ev.key().as_str() {
                        "Enter" => {
                            ev.prevent_default();
                            if let Some((username, _)) = hits.with(|(list, _)| list.first().cloned()) {
                                pick(username);
                            }
                        }
                        "Escape" => open.set(false),
                        _ => {}
                    }
                }
            />
            <Show when=move || open.get()>
                // Mausdruck nicht ans Feld weitergeben: Das Feld behält den Fokus, bis der Tipp angekommen ist.
                <ul class="person-hits" role="listbox" data-noswipe="" on:mousedown=move |ev| ev.prevent_default()>
                    {move || {
                        let (list, more) = hits.get();
                        if list.is_empty() {
                            return view! { <li class="empty">"Niemand gefunden."</li> }.into_any();
                        }
                        let rows = list
                            .into_iter()
                            .map(|(username, name)| {
                                view! {
                                    <li>
                                        <button class="person-hit" role="option" on:click=move |_| pick(username.clone())>
                                            {name}
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view();
                        view! {
                            {rows}
                            {(more > 0)
                                .then(|| view! { <li class="muted person-more">{format!("und {more} weitere, tippe mehr vom Namen")}</li> })}
                        }
                            .into_any()
                    }}
                </ul>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::api::Me;

    fn users(n: usize) -> Vec<Me> {
        (0..n)
            .map(|i| Me {
                username: format!("user{i}"),
                display_name: format!("Person Nummer{i}"),
                avatar: 0,
                admin: false,
            })
            .collect()
    }

    #[test]
    fn the_menu_shows_a_few_hits_and_counts_the_rest() {
        let users = users(20);
        let people = short_names(&users);
        let (list, more) = pick_hits(&people, &users, "", &BTreeSet::new());
        assert_eq!((list.len(), more), (HITS, 12));
        let (found, more) = pick_hits(&people, &users, "nummer7", &BTreeSet::new());
        assert_eq!((found.len(), more), (1, 0));
        assert_eq!(found[0].0, "user7");
    }

    #[test]
    fn chosen_people_do_not_show_up_again() {
        let users = users(3);
        let people = short_names(&users);
        let taken: BTreeSet<String> = ["user1".to_owned()].into();
        let (list, _) = pick_hits(&people, &users, "", &taken);
        assert_eq!(
            list.iter().map(|(u, _)| u.as_str()).collect::<Vec<_>>(),
            ["user0", "user2"]
        );
    }
}
