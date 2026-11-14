//! Blätterleiste für lange Listen: Es wird immer nur eine Seite gezeigt, nie die ganze Liste. Die Leiste steht oben und
//! unten, damit man nicht erst ans Ende scrollen muss.

use leptos::prelude::*;

use crate::filter;
use crate::icons::Icon;
use crate::jsutil;

/// Text zwischen den Knöpfen: `21 bis 40 von 87 (Seite 2 von 5)`.
pub fn range_text(start: usize, end: usize, total: usize, size: usize, pages: usize) -> String {
    format!(
        "{} bis {} von {} (Seite {} von {})",
        start + 1,
        end,
        total,
        start / size.max(1) + 1,
        pages
    )
}

/// Blätterleiste mit Zurück und Weiter. Sie ist nur da, wenn es mehr als eine Seite gibt.
/// `anchor` ist die Kennung des Elements, zu dem nach dem Blättern gescrollt wird (die Filterleiste der Liste).
#[component]
pub fn Pager(
    total: Signal<usize>,
    page: RwSignal<usize>,
    size: usize,
    label: &'static str,
    anchor: &'static str,
    /// Leiste über der Liste statt darunter (Rand auf der anderen Seite).
    #[prop(optional)]
    top: bool,
) -> impl IntoView {
    let bounds = move || filter::page_bounds(total.get(), page.get(), size);
    let several = move || bounds().2 > 1;
    // Vergleiche stehen außerhalb von view!, ein ">" im Attribut beendet dort das Tag.
    let at_start = move || bounds().0 == 0;
    let at_end = move || bounds().1 >= total.get();
    let go = move |target: usize| {
        page.set(target);
        jsutil::scroll_to_id(anchor);
    };
    view! {
        <Show when=several>
            <nav class=if top { "pager pager-top" } else { "pager" } aria-label=label>
                <button
                    class="btn"
                    disabled=at_start
                    on:click=move |_| go(page.get_untracked().min(bounds().2 - 1).saturating_sub(1))
                >
                    <Icon name="chevron-left"/>
                    "Zurück"
                </button>
                <span class="pager-text mono-num">
                    {move || {
                        let (start, end, pages) = bounds();
                        range_text(start, end, total.get(), size, pages)
                    }}
                </span>
                <button
                    class="btn"
                    disabled=at_end
                    on:click=move |_| go(page.get_untracked().min(bounds().2 - 1) + 1)
                >
                    "Weiter"
                    <Icon name="chevron-right"/>
                </button>
            </nav>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_reads_naturally() {
        assert_eq!(range_text(0, 20, 87, 20, 5), "1 bis 20 von 87 (Seite 1 von 5)");
        assert_eq!(range_text(80, 87, 87, 20, 5), "81 bis 87 von 87 (Seite 5 von 5)");
        assert_eq!(range_text(10, 20, 25, 10, 3), "11 bis 20 von 25 (Seite 2 von 3)");
    }
}
