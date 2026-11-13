//! Ein Hinweis, den man auf dem Handy mit dem Finger wegwischt: nach links, nach rechts oder nach oben.
//! Solange der Finger auf dem Glas liegt, folgt der Hinweis ihm. Die Entscheidung trifft `gesture::dismisses`.

use leptos::prelude::*;

use crate::gesture;

/// Position des ersten Fingers. Beim Loslassen stehen die Finger nicht mehr in `touches`, sondern in
/// `changedTouches`.
pub fn touch_point(ev: &web_sys::TouchEvent, ended: bool) -> Option<(f64, f64)> {
    let list = if ended { ev.changed_touches() } else { ev.touches() };
    list.get(0).map(|t| (f64::from(t.client_x()), f64::from(t.client_y())))
}

#[component]
pub fn Swipeable(
    class: &'static str,
    role: &'static str,
    #[prop(into)] on_dismiss: Callback<()>,
    children: Children,
) -> impl IntoView {
    let start = StoredValue::new(None::<(f64, f64)>);
    let moved = RwSignal::new((0.0_f64, 0.0_f64));
    let reset = move || {
        start.set_value(None);
        moved.set((0.0, 0.0));
    };
    view! {
        <div
            class=class
            role=role
            // Wischen auf dem Hinweis wechselt nicht die Seite.
            data-noswipe=""
            style=move || {
                let (dx, dy) = moved.get();
                let (x, y) = gesture::drag_offset(dx, dy);
                if x == 0.0 && y == 0.0 {
                    String::new()
                } else {
                    format!(
                        "transform: translate({x:.0}px, {y:.0}px); opacity: {:.2}; transition: none; animation: none",
                        gesture::drag_opacity(dx, dy),
                    )
                }
            }
            on:touchstart=move |ev| start.set_value(touch_point(&ev, false))
            on:touchmove=move |ev| {
                if let (Some((sx, sy)), Some((x, y))) = (start.get_value(), touch_point(&ev, false)) {
                    moved.set((x - sx, y - sy));
                }
            }
            on:touchend=move |ev| {
                let delta = match (start.get_value(), touch_point(&ev, true)) {
                    (Some((sx, sy)), Some((x, y))) => (x - sx, y - sy),
                    _ => (0.0, 0.0),
                };
                reset();
                if gesture::dismisses(delta.0, delta.1) {
                    on_dismiss.run(());
                }
            }
            on:touchcancel=move |_| reset()
        >
            {children()}
        </div>
    }
}
