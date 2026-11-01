//! Ladestandsverlauf als Diagramm: charming (Rust-Wrapper für Apache ECharts).

use charming::component::{Axis, Grid};
use charming::element::{
    AxisLabel, AxisType, Label, LabelPosition, LineStyle, MarkLine, MarkLineData, MarkLineVariant, Tooltip, Trigger,
};
use charming::series::Line;
use charming::{Chart, WasmRenderer};
use leptos::html;
use leptos::prelude::*;
use shared::api::HistoryPoint;

fn build(points: &[HistoryPoint], threshold: Option<f64>) -> Chart {
    let data: Vec<Vec<f64>> = points.iter().map(|p| vec![p.t_ms as f64, p.charge_pct]).collect();

    let mut series = Line::new()
        .show_symbol(false)
        .smooth(false)
        .line_style(LineStyle::new().width(2))
        .data(data);
    if let Some(limit) = threshold {
        series = series.mark_line(
            MarkLine::new()
                .silent(true)
                .symbol(vec![charming::element::Symbol::None, charming::element::Symbol::None])
                .line_style(
                    LineStyle::new()
                        .type_(charming::element::LineStyleType::Dashed)
                        .color("#c84d38"),
                )
                .label(
                    Label::new()
                        .position(LabelPosition::InsideEndTop)
                        .formatter(format!("Schwellwert {limit} %").as_str()),
                )
                .data(vec![MarkLineVariant::Simple(MarkLineData::new().y_axis(limit))]),
        );
    }

    Chart::new()
        .animation(false)
        .color(vec!["#d98a2b"])
        .tooltip(Tooltip::new().trigger(Trigger::Axis))
        .grid(Grid::new().left(48).right(16).top(16).bottom(32))
        .x_axis(Axis::new().type_(AxisType::Time))
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .min(0)
                .max(100)
                .axis_label(AxisLabel::new().formatter("{value} %")),
        )
        .series(series)
}

/// Zeichnet den Verlauf neu, sobald sich Werte oder Schwellwert ändern.
#[component]
pub fn ChargeChart(
    #[prop(into)] points: Signal<Vec<HistoryPoint>>,
    #[prop(into)] threshold: Signal<Option<f64>>,
) -> impl IntoView {
    let node = NodeRef::<html::Div>::new();
    // Zählt Größenänderungen und Wiederholungen hoch und löst damit ein neues Zeichnen aus.
    let redraw = RwSignal::new(0u32);
    window_event_listener(leptos::ev::resize, move |_| redraw.update(|n| *n = n.wrapping_add(1)));

    Effect::new(move |_| {
        redraw.track();
        let (points, threshold) = (points.get(), threshold.get());
        let Some(element) = node.get() else { return };
        let width = element.client_width();
        if width < 50 {
            // Das Layout steht noch nicht: im nächsten Bildschirmupdate erneut versuchen.
            request_animation_frame(move || redraw.update(|n| *n = n.wrapping_add(1)));
            return;
        }
        let chart = build(&points, threshold);
        let width = width as u32;
        if let Err(e) = WasmRenderer::new(width, 320).render("charge-chart", &chart) {
            leptos::logging::error!("Diagramm konnte nicht gezeichnet werden: {e:?}");
        }
    });

    view! { <div id="charge-chart" class="chart" node_ref=node></div> }
}
