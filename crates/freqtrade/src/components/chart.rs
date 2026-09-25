//! Cumulative profit chart.
//!
//! Inline SVG, no charting library. The server already computed the series
//! (`ft_server::compute::cumulative_series`), so this only has to turn numbers
//! into geometry.
//!
//! A step line rather than a smooth one, because that is what the data means:
//! cumulative profit changes discretely the moment a trade closes and is flat
//! in between. Interpolating would draw profit accruing at times when nothing
//! happened.

use dioxus::prelude::*;

use ft_types::api::{CumulativeSeries, SeriesUnit};

use crate::format::{self, Tone};

// Geometry in viewBox units; CSS scales the whole thing.
const W: f64 = 1000.0;
const H: f64 = 320.0;
const PAD_L: f64 = 68.0;
const PAD_R: f64 = 14.0;
const PAD_T: f64 = 16.0;
const PAD_B: f64 = 26.0;

const PLOT_W: f64 = W - PAD_L - PAD_R;
const PLOT_H: f64 = H - PAD_T - PAD_B;

#[component]
pub fn ProfitChart(series: CumulativeSeries) -> Element {
    // Width of the rendered element, needed to map a pointer position back to
    // a data index. The SVG scales, so CSS pixels and viewBox units differ.
    let mut rendered_width = use_signal(|| 0.0_f64);
    let mut hover = use_signal(|| None::<usize>);

    let points = series.points.clone();
    if points.len() < 2 {
        return rsx! {
            div { class: "chart chart--empty",
                div { class: "empty__icon", "📈" }
                p { class: "muted", "Not enough closed trades to plot yet." }
            }
        };
    }

    let values: Vec<f64> = points.iter().map(|p| p.cumulative).collect();
    let last = *values.last().unwrap_or(&0.0);
    let tone = Tone::of(last);

    // Include zero in the range so the baseline is always meaningful, and pad
    // slightly so the line never sits exactly on the frame.
    let data_min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let data_max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    // Zero is always in range, so the curve is read against break-even rather
    // than against whatever its own worst point happened to be.
    let (raw_min, raw_max) = (data_min.min(0.0), data_max.max(0.0));
    let span = (raw_max - raw_min).max(f64::EPSILON);
    let pad = span * 0.08;

    // Pad outward only where the data actually goes. Padding past zero on a
    // series that never lost money draws an axis tick in negative territory
    // and implies drawdowns that never happened.
    let min = if data_min < 0.0 {
        raw_min - pad
    } else {
        raw_min
    };
    let max = if data_max > 0.0 {
        raw_max + pad
    } else {
        raw_max
    };
    let range = (max - min).max(f64::EPSILON);

    let x_at = |i: usize| PAD_L + PLOT_W * (i as f64) / ((points.len() - 1) as f64);
    let y_at = |v: f64| PAD_T + PLOT_H * (1.0 - (v - min) / range);

    // Step-after: hold the previous value until the next trade closes.
    let mut line = String::with_capacity(points.len() * 18);
    for (i, value) in values.iter().enumerate() {
        let (x, y) = (x_at(i), y_at(*value));
        if i == 0 {
            line.push_str(&format!("M{x:.1},{y:.1}"));
        } else {
            let prev_y = y_at(values[i - 1]);
            line.push_str(&format!("L{x:.1},{prev_y:.1}L{x:.1},{y:.1}"));
        }
    }
    let baseline_y = y_at(0.0);
    let area = format!(
        "{line}L{:.1},{baseline_y:.1}L{:.1},{baseline_y:.1}Z",
        x_at(points.len() - 1),
        PAD_L
    );

    // Four gridlines, labelled. Recessive: the data is the subject.
    let ticks: Vec<(f64, String)> = (0..=3)
        .map(|i| {
            let value = min + range * (i as f64 / 3.0);
            (y_at(value), tick_label(value, &series))
        })
        .collect();

    let first_label = format::datetime(points[0].time);
    let last_label = format::datetime(points[points.len() - 1].time);

    let hovered = hover();
    let hover_point = hovered.and_then(|i| points.get(i).copied());

    let point_count = points.len();
    let track = move |event: PointerEvent| {
        let width = *rendered_width.peek();
        if width <= 0.0 {
            return;
        }
        // Element coordinates are CSS pixels; convert to a data index via the
        // plot's share of the viewBox.
        let x_css = event.data.element_coordinates().x;
        let x_view = x_css / width * W;
        let fraction = ((x_view - PAD_L) / PLOT_W).clamp(0.0, 1.0);
        let index = (fraction * (point_count - 1) as f64).round() as usize;
        hover.set(Some(index.min(point_count - 1)));
    };

    rsx! {
        div { class: "chart",
            svg {
                class: "chart__svg",
                view_box: "0 0 {W} {H}",
                preserve_aspect_ratio: "none",
                onmounted: move |event| async move {
                    if let Ok(rect) = event.get_client_rect().await {
                        rendered_width.set(rect.width());
                    }
                },
                onpointermove: track,
                onpointerleave: move |_| hover.set(None),

                defs {
                    linearGradient { id: "chart-fill", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%", class: "chart__stop chart__stop--{tone.class()}" }
                        stop { offset: "100%", class: "chart__stop chart__stop--fade" }
                    }
                }

                // Grid first, so the data draws over it.
                for (y, label) in ticks.iter() {
                    g { key: "{label}",
                        line {
                            class: "chart__grid",
                            x1: "{PAD_L}", y1: "{y}", x2: "{W - PAD_R}", y2: "{y}",
                        }
                        text {
                            class: "chart__tick",
                            x: "{PAD_L - 10.0}", y: "{y + 4.0}",
                            text_anchor: "end",
                            "{label}"
                        }
                    }
                }

                // Zero is the line that decides profit from loss, so it is
                // drawn distinctly from the ordinary grid.
                if data_min < 0.0 && data_max > 0.0 {
                    line {
                        class: "chart__zero",
                        x1: "{PAD_L}", y1: "{baseline_y}", x2: "{W - PAD_R}", y2: "{baseline_y}",
                    }
                }

                path { class: "chart__area", d: "{area}" }
                path {
                    class: "chart__line chart__line--{tone.class()}",
                    d: "{line}",
                    vector_effect: "non-scaling-stroke",
                }

                if let (Some(index), Some(point)) = (hovered, hover_point) {
                    g {
                        line {
                            class: "chart__crosshair",
                            x1: "{x_at(index)}", y1: "{PAD_T}",
                            x2: "{x_at(index)}", y2: "{PAD_T + PLOT_H}",
                            vector_effect: "non-scaling-stroke",
                        }
                        circle {
                            class: "chart__dot chart__dot--{tone.class()}",
                            cx: "{x_at(index)}", cy: "{y_at(point.cumulative)}", r: "6",
                        }
                    }
                }

                text { class: "chart__axis", x: "{PAD_L}", y: "{H - 6.0}", "{first_label}" }
                text {
                    class: "chart__axis",
                    x: "{W - PAD_R}", y: "{H - 6.0}",
                    text_anchor: "end",
                    "{last_label}"
                }
            }

            if let Some(point) = hover_point {
                div {
                    class: "chart__tip",
                    // Anchored by percentage so it tracks the scaled SVG.
                    style: "left: {(x_at(hovered.unwrap_or(0)) / W * 100.0):.2}%",
                    div { class: "chart__tip-value chart__tip-value--{tone.class()}",
                        "{label_for(point.cumulative, &series)}"
                    }
                    div { class: "chart__tip-time", "{format::datetime(point.time)}" }
                }
            }
        }
    }
}

/// A compact axis label: the number without its unit.
///
/// Repeating "USDT" on every gridline is noise, and at four gridlines it was
/// wide enough to overflow the card. The unit is already stated by the hero
/// figure above the chart and by the tooltip.
fn tick_label(value: f64, series: &CumulativeSeries) -> String {
    match series.unit {
        SeriesUnit::Percent => format::percent(value / 100.0),
        SeriesUnit::Currency => format::signed_money(value, ""),
    }
}

/// Formats a series value in its own unit.
///
/// The unit is explicit in the payload because Freqtrade only knows
/// `starting_capital` on some configurations; without it a percentage would be
/// meaningless and the server falls back to the stake currency.
fn label_for(value: f64, series: &CumulativeSeries) -> String {
    match series.unit {
        SeriesUnit::Percent => format::percent(value / 100.0),
        SeriesUnit::Currency => format::signed_money(value, &series.currency),
    }
}
