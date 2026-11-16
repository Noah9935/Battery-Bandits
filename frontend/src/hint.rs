//! Kleines Fragezeichen mit aufklappbarer Erklärung. So steht der erklärende Text nicht mehr dauernd auf der Seite,
//! sondern nur, wenn jemand ihn braucht. Ein Tipp öffnet ihn, ein Tipp daneben oder Esc schließt ihn.

use leptos::html;
use leptos::prelude::*;

use crate::icons::Icon;

/// Fragezeichen-Knopf mit Erklärung. `end` richtet die Sprechblase am rechten Rand aus (für Hinweise am rechten
/// Ende einer Zeile, damit sie nicht aus dem Bild ragt).
#[component]
pub fn Hint(#[prop(into)] text: String, #[prop(optional)] end: bool) -> impl IntoView {
    let open = RwSignal::new(false);
    let root = NodeRef::<html::Div>::new();
    crate::forward::close_on_outside_click(root, open);
    let pop_text = text.clone();
    view! {
        <div
            class="hint"
            node_ref=root
            on:keydown=move |ev| {
                if ev.key() == "Escape" && open.get_untracked() {
                    ev.stop_propagation();
                    open.set(false);
                }
            }
        >
            <button
                type="button"
                class=move || if open.get() { "hint-btn is-on" } else { "hint-btn" }
                aria-label="Erklärung anzeigen"
                aria-expanded=move || open.get().to_string()
                title=text
                on:click=move |ev| {
                    ev.stop_propagation();
                    open.update(|o| *o = !*o);
                }
            >
                <Icon name="circle-help"/>
            </button>
            <Show when=move || open.get()>
                <span class=if end { "hint-pop hint-end" } else { "hint-pop" } role="note">{pop_text.clone()}</span>
            </Show>
        </div>
    }
}
