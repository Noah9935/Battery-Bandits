//! Lange Listen zeigen zuerst einen Teil und laden auf Wunsch weitere nach. So wird die Seite nicht schwer, wenn es
//! viele Einträge gibt, und nichts springt: Neue Einträge erscheinen unter den bisherigen.

use leptos::prelude::*;

/// So viele Einträge kommen mit jedem Tipp auf "Weitere anzeigen" dazu.
pub const STEP: usize = 20;

/// Wie viele Einträge nach einem Tipp sichtbar sind.
pub fn next_visible(shown: usize, total: usize, step: usize) -> usize {
    (shown + step.max(1)).min(total.max(shown))
}

/// Wie viele Einträge noch fehlen.
pub fn remaining(shown: usize, total: usize) -> usize {
    total.saturating_sub(shown)
}

/// Knopf unter einer Liste: `Weitere 20 anzeigen (20 von 87)`. Erscheint nur, wenn noch etwas fehlt.
#[component]
pub fn MoreButton(
    #[prop(into)] total: Signal<usize>,
    shown: RwSignal<usize>,
    #[prop(default = STEP)] step: usize,
) -> impl IntoView {
    let missing = move || remaining(shown.get(), total.get());
    let any_missing = move || missing() > 0;
    view! {
        <Show when=any_missing>
            <div class="more">
                <button class="btn" on:click=move |_| shown.update(|n| *n = next_visible(*n, total.get_untracked(), step))>
                    {move || format!("Weitere {} anzeigen ({} von {})", missing().min(step), shown.get().min(total.get()), total.get())}
                </button>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tap_adds_a_step_but_never_beyond_the_total() {
        assert_eq!(next_visible(20, 87, 20), 40);
        assert_eq!(next_visible(80, 87, 20), 87);
        assert_eq!(next_visible(87, 87, 20), 87);
        assert_eq!(next_visible(20, 5, 20), 20, "schon alles sichtbar: nichts ändert sich");
        assert_eq!(next_visible(0, 10, 0), 1, "Schritt 0 gilt als 1");
    }

    #[test]
    fn the_button_knows_how_many_are_missing() {
        assert_eq!(remaining(20, 87), 67);
        assert_eq!(remaining(87, 87), 0);
        assert_eq!(remaining(100, 87), 0);
        assert_eq!(STEP, 20);
    }
}
