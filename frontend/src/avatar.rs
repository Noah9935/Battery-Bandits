//! Profilbilder: echte Waschbär-Fotos. Jedes Konto bekommt bei der Anlage zufällig eine Nummer zugewiesen (auf dem
//! Server) und kann sie in den Optionen wechseln. Die Fotos liegen quadratisch zugeschnitten unter
//! `frontend/avatars/00.jpg` und so weiter, Urheber und Lizenz stehen in `frontend/avatars/BILDNACHWEIS.txt`
//! (in den Optionen verlinkt).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::icons::Icon;
use crate::state::Store;

/// Zahl der Bilder.
pub const COUNT: u8 = shared::api::AVATAR_COUNT;

/// Adresse des Fotos mit dieser Nummer. Nummern nach der letzten beginnen von vorn.
pub fn avatar_url(id: u8) -> String {
    format!("/avatars/{:02}.jpg", id % COUNT)
}

/// Die Nummer eine Stelle weiter oder zurück. Nach dem letzten Bild geht es von vorn los.
pub fn step(current: u8, forward: bool) -> u8 {
    let (count, current) = (u16::from(COUNT), u16::from(current) % u16::from(COUNT));
    (if forward { current + 1 } else { current + count - 1 } % count) as u8
}

/// Ein zufälliges Bild, aber nie das aktuelle. `roll` ist eine Zufallszahl von 0 (einschließlich) bis 1 (ausschließlich).
pub fn random_other(current: u8, roll: f64) -> u8 {
    let others = COUNT - 1;
    let pick = ((roll.clamp(0.0, 1.0) * f64::from(others)) as u8).min(others - 1);
    let current = current % COUNT;
    if pick >= current { pick + 1 } else { pick }
}

/// Auswahl in den Optionen: durch die Fotos blättern oder würfeln, mit "Übernehmen" speichern.
#[component]
pub fn AvatarPicker() -> impl IntoView {
    let s = expect_context::<Store>();
    let saved = move || s.me.with(|me| me.as_ref().map_or(0, |me| me.avatar % COUNT));
    let draft = RwSignal::new(saved());
    let busy = RwSignal::new(false);
    let changed = move || draft.get() != saved();
    let save = move |_| {
        if busy.get_untracked() {
            return;
        }
        let wanted = draft.get_untracked();
        busy.set(true);
        spawn_local(async move {
            match api::set_avatar(wanted).await {
                Ok(me) => {
                    s.me.set(Some(me));
                    s.say("Profilbild geändert.");
                }
                Err(e) => s.say(e),
            }
            busy.set(false);
        });
    };
    view! {
        <section id="opt-avatar" class="surface surface-pad avatar-picker">
            <span class="avatar-img picker-avatar" aria-hidden="true">
                <img src=move || avatar_url(draft.get()) alt="" width="192" height="192" decoding="async"/>
            </span>
            <div class="picker-main">
                <h2>"Profilbild"</h2>
                <div class="picker-controls">
                    <button
                        class="icon-btn"
                        aria-label="Vorheriges Bild"
                        on:click=move |_| draft.update(|d| *d = step(*d, false))
                    >
                        <Icon name="chevron-left"/>
                    </button>
                    <span class="picker-count">
                        {move || format!("Bild {} von {}", u16::from(draft.get()) + 1, COUNT)}
                    </span>
                    <button
                        class="icon-btn"
                        aria-label="Nächstes Bild"
                        on:click=move |_| draft.update(|d| *d = step(*d, true))
                    >
                        <Icon name="chevron-right"/>
                    </button>
                    <button
                        class="btn"
                        on:click=move |_| draft.update(|d| *d = random_other(*d, js_sys::Math::random()))
                    >
                        <Icon name="shuffle"/>
                        "Zufällig"
                    </button>
                    <button class="btn btn-primary" disabled=move || busy.get() || !changed() on:click=save>
                        <Icon name="check"/>
                        "Übernehmen"
                    </button>
                </div>
            </div>
        </section>
    }
}

/// Rundes Profilbild.
#[component]
pub fn Avatar(id: u8, #[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <span class=format!("avatar-img {class}") aria-hidden="true">
            <img src=avatar_url(id) alt="" width="96" height="96" loading="lazy" decoding="async"/>
        </span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_number_has_its_own_photo() {
        let all: BTreeSet<String> = (0..COUNT).map(avatar_url).collect();
        assert_eq!(all.len(), usize::from(COUNT));
        assert_eq!(COUNT, 164);
        assert_eq!(avatar_url(0), "/avatars/00.jpg");
        assert_eq!(avatar_url(99), "/avatars/99.jpg");
        assert_eq!(avatar_url(163), "/avatars/163.jpg");
    }

    #[test]
    fn numbers_beyond_the_last_start_over() {
        assert_eq!(avatar_url(COUNT), avatar_url(0));
        assert_eq!(avatar_url(255), avatar_url(255 - COUNT));
    }

    #[test]
    fn stepping_wraps_around_at_both_ends() {
        assert_eq!(step(0, true), 1);
        assert_eq!(step(COUNT - 1, true), 0);
        assert_eq!(step(0, false), COUNT - 1);
        assert_eq!(step(5, false), 4);
        assert_eq!(
            step(255, true),
            (255 % COUNT + 1) % COUNT,
            "auch bei unsinnigen Nummern kein Überlauf"
        );
    }

    #[test]
    fn the_dice_never_repeat_the_current_picture_and_reach_every_other_one() {
        for current in [0, 1, 80, COUNT - 1] {
            let mut seen = BTreeSet::new();
            for k in 0..=1000u32 {
                let pick = random_other(current, f64::from(k) / 1000.0);
                assert!(pick < COUNT);
                assert_ne!(pick, current, "Wurf {k}");
                seen.insert(pick);
            }
            assert_eq!(
                seen.len(),
                usize::from(COUNT) - 1,
                "jedes andere Bild ist erreichbar (Start {current})"
            );
        }
    }

    #[test]
    fn the_photos_and_their_credits_are_part_of_the_project() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("avatars");
        for id in 0..COUNT {
            let file = dir.join(format!("{id:02}.jpg"));
            let bytes = std::fs::read(&file).unwrap_or_else(|_| panic!("{} fehlt", file.display()));
            assert!(bytes.starts_with(&[0xFF, 0xD8]), "{} ist kein JPEG", file.display());
        }
        let credits = std::fs::read_to_string(dir.join("BILDNACHWEIS.txt")).expect("Bildnachweis fehlt");
        assert_eq!(
            credits.lines().filter(|l| l.starts_with("Bild ")).count(),
            usize::from(COUNT),
            "je Bild ein Eintrag"
        );
    }
}
