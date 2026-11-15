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

/// Die Seitenzahlen, die als Knöpfe erscheinen (null-basiert, `None` ist eine Lücke "…"): Am Anfang die ersten fünf
/// Seiten und die letzte, am Ende die letzten fünf und die erste, dazwischen die Nachbarn der aktuellen Seite.
pub fn page_links(current: usize, pages: usize) -> Vec<Option<usize>> {
    if pages <= 7 {
        return (0..pages).map(Some).collect();
    }
    let last = pages - 1;
    let current = current.min(last);
    if current < 4 {
        let mut v: Vec<Option<usize>> = (0..5).map(Some).collect();
        v.extend([None, Some(last)]);
        v
    } else if current > last - 4 {
        let mut v = vec![Some(0), None];
        v.extend((last - 4..=last).map(Some));
        v
    } else {
        vec![
            Some(0),
            None,
            Some(current - 1),
            Some(current),
            Some(current + 1),
            None,
            Some(last),
        ]
    }
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
    // Die Leiste oben bleibt, wo sie ist: Sonst springt die Seite bei jedem Blättern, weil die Liste darunter
    // mal länger und mal kürzer ist. Nur die Leiste unten führt zurück an den Anfang der Liste.
    let go = move |target: usize| {
        page.set(target);
        if !top {
            jsutil::scroll_to_id(anchor);
        }
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
                <div class="pager-pages" role="group" aria-label="Seite wählen">
                    {move || {
                        let current = page.get().min(bounds().2 - 1);
                        page_links(current, bounds().2)
                            .into_iter()
                            .map(|link| match link {
                                Some(n) => {
                                    let is_now = n == current;
                                    view! {
                                        <button
                                            class=if is_now { "pager-page is-on" } else { "pager-page" }
                                            aria-current=if is_now { "page" } else { "false" }
                                            aria-label=format!("Seite {}", n + 1)
                                            on:click=move |_| go(n)
                                        >
                                            {n + 1}
                                        </button>
                                    }
                                        .into_any()
                                }
                                None => view! { <span class="pager-gap" aria-hidden="true">"…"</span> }.into_any(),
                            })
                            .collect_view()
                    }}
                </div>
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
    fn the_page_buttons_show_the_first_five_and_the_last() {
        let n = |v: Vec<Option<usize>>| v.into_iter().map(|x| x.map_or(0, |p| p + 1)).collect::<Vec<_>>();
        assert_eq!(n(page_links(0, 2)), [1, 2]);
        assert_eq!(n(page_links(0, 12)), [1, 2, 3, 4, 5, 0, 12]);
        assert_eq!(n(page_links(3, 12)), [1, 2, 3, 4, 5, 0, 12]);
        assert_eq!(n(page_links(6, 12)), [1, 0, 6, 7, 8, 0, 12]);
        assert_eq!(n(page_links(11, 12)), [1, 0, 8, 9, 10, 11, 12]);
        assert_eq!(page_links(0, 7).len(), 7);
    }

    #[test]
    fn the_range_reads_naturally() {
        assert_eq!(range_text(0, 20, 87, 20, 5), "1 bis 20 von 87 (Seite 1 von 5)");
        assert_eq!(range_text(80, 87, 87, 20, 5), "81 bis 87 von 87 (Seite 5 von 5)");
        assert_eq!(range_text(10, 20, 25, 10, 3), "11 bis 20 von 25 (Seite 2 von 3)");
    }
}
