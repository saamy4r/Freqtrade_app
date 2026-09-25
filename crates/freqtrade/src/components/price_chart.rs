//! Price chart with trade markers and a pannable window.
//!
//! The legacy screen carried a candlestick icon but only ever drew a close
//! price line; this draws the line too, for the same reason — at a hundred
//! bars on a phone, wicks are narrower than a finger and add noise rather than
//! information. Full OHLC is decoded and available (`ft_types::Candle`) if that
//! judgement turns out to be wrong.

use dioxus::prelude::*;

use ft_types::api::TradeOverlay;
use ft_types::freqtrade::Candle;

use crate::format::{self, Tone};

const W: f64 = 1000.0;
const H: f64 = 420.0;
const PAD_L: f64 = 12.0;
const PAD_R: f64 = 72.0; // price labels sit on the right, as on a trading chart
const PAD_T: f64 = 14.0;
const PAD_B: f64 = 28.0;
const PLOT_W: f64 = W - PAD_L - PAD_R;
const PLOT_H: f64 = H - PAD_T - PAD_B;

/// Bars visible at once, matching the legacy window.
const WINDOW: usize = 100;

#[component]
pub fn PriceChart(candles: Vec<Candle>, overlays: Vec<TradeOverlay>) -> Element {
    // Offset of the window's first bar. `None` means "the most recent bars",
    // which is where a price chart should open and which cannot be expressed
    // as a number before the candle count is known.
    let mut offset = use_signal(|| None::<usize>);
    let mut drag_from = use_signal(|| None::<(f64, usize)>);
    let mut rendered_width = use_signal(|| 0.0_f64);

    let total = candles.len();
    if total < 2 {
        return rsx! {
            div { class: "chart chart--empty",
                div { class: "empty__icon", "📉" }
                p { class: "muted", "No candles for this pair yet." }
            }
        };
    }

    let window = WINDOW.min(total);
    let max_offset = total - window;
    // A subscribing read: panning has to re-render. `peek` here would set the
    // signal on click and never redraw, leaving the buttons visibly dead.
    // Clamped on every render, so switching to a pair with fewer bars cannot
    // leave the window scrolled past the end.
    let start = offset().unwrap_or(max_offset).min(max_offset);
    let visible = &candles[start..start + window];

    let (from_time, to_time) = (visible[0].time, visible[window - 1].time);
    let lo = visible.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let hi = visible
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    // Markers can sit outside the visible price range; include them so an
    // entry never ends up drawn off the top of the chart.
    let (lo, hi) = overlays
        .iter()
        .filter(|o| o.entry_time >= from_time && o.entry_time <= to_time)
        .fold((lo, hi), |(lo, hi), o| {
            (lo.min(o.entry_price), hi.max(o.entry_price))
        });
    let pad = ((hi - lo) * 0.06).max(f64::EPSILON);
    let (lo, hi) = (lo - pad, hi + pad);
    let range = (hi - lo).max(f64::EPSILON);

    let span = (to_time - from_time).max(1) as f64;
    let x_at_time = move |t: i64| PAD_L + PLOT_W * ((t - from_time) as f64 / span);
    let y_at = move |p: f64| PAD_T + PLOT_H * (1.0 - (p - lo) / range);

    let mut line = String::with_capacity(window * 14);
    for (i, candle) in visible.iter().enumerate() {
        let (x, y) = (x_at_time(candle.time), y_at(candle.close));
        line.push_str(&format!("{}{x:.1},{y:.1}", if i == 0 { "M" } else { "L" }));
    }

    let ticks: Vec<(f64, String)> = (0..=3)
        .map(|i| {
            let price = lo + range * (i as f64 / 3.0);
            (y_at(price), format::price(price))
        })
        .collect();

    // Only trades whose entry falls in the window; an exit-only marker with no
    // visible entry reads as a stray dot.
    let drawn: Vec<&TradeOverlay> = overlays
        .iter()
        .filter(|o| {
            (o.entry_time >= from_time && o.entry_time <= to_time)
                || o.exit_time.is_some_and(|t| t >= from_time && t <= to_time)
        })
        .collect();

    let at_start = start == 0;
    let at_end = start >= max_offset;

    let begin_drag = move |event: PointerEvent| {
        let x = event.data.element_coordinates().x;
        drag_from.set(Some((
            x,
            offset.peek().unwrap_or(max_offset).min(max_offset),
        )));
    };
    let on_drag = move |event: PointerEvent| {
        let Some((from_x, from_offset)) = *drag_from.peek() else {
            return;
        };
        let width = *rendered_width.peek();
        if width <= 0.0 {
            return;
        }
        // A drag of one plot-width moves the window by one full page.
        let dx = event.data.element_coordinates().x - from_x;
        let bars = (dx / width * W / PLOT_W * window as f64).round() as i64;
        // Dragging right reveals older bars, as on every trading chart.
        let next = (from_offset as i64 - bars).clamp(0, max_offset as i64);
        offset.set(Some(next as usize));
    };
    let end_drag = move |_| drag_from.set(None);

    rsx! {
        div { class: "chart chart--price",
            svg {
                class: "chart__svg chart__svg--price",
                view_box: "0 0 {W} {H}",
                preserve_aspect_ratio: "none",
                onmounted: move |event| async move {
                    if let Ok(rect) = event.get_client_rect().await {
                        rendered_width.set(rect.width());
                    }
                },
                onpointerdown: begin_drag,
                onpointermove: on_drag,
                onpointerup: end_drag,
                onpointerleave: end_drag,

                for (y, label) in ticks.iter() {
                    g { key: "{label}",
                        line {
                            class: "chart__grid",
                            x1: "{PAD_L}", y1: "{y}", x2: "{W - PAD_R}", y2: "{y}",
                        }
                        text {
                            class: "chart__tick",
                            x: "{W - PAD_R + 8.0}", y: "{y + 5.0}",
                            "{label}"
                        }
                    }
                }

                path {
                    class: "chart__line chart__line--price",
                    d: "{line}",
                    vector_effect: "non-scaling-stroke",
                }

                for overlay in drawn.iter() {
                    TradeMarks {
                        key: "{overlay.trade_id}",
                        overlay: (*overlay).clone(),
                        entry_x: x_at_time(overlay.entry_time),
                        entry_y: y_at(overlay.entry_price),
                        exit: overlay.exit_time.zip(overlay.exit_price)
                            .map(|(t, p)| (x_at_time(t), y_at(p))),
                    }
                }

                text { class: "chart__axis", x: "{PAD_L}", y: "{H - 8.0}", "{format::datetime(from_time)}" }
                text {
                    class: "chart__axis",
                    x: "{W - PAD_R}", y: "{H - 8.0}",
                    text_anchor: "end",
                    "{format::datetime(to_time)}"
                }
            }

            div { class: "chart__pan",
                button {
                    class: "chart__pan-btn",
                    disabled: at_start,
                    onclick: move |_| offset.set(Some(start.saturating_sub(window / 2))),
                    "← older"
                }
                span { class: "muted", "{start + 1}–{start + window} of {total}" }
                button {
                    class: "chart__pan-btn",
                    disabled: at_end,
                    onclick: move |_| offset.set(Some((start + window / 2).min(max_offset))),
                    "newer →"
                }
            }

            // Identity is never colour alone: each mark is named here, and the
            // shapes differ as well as the hues.
            div { class: "legend",
                LegendItem { mark: "line", tone: "price", label: "Close price" }
                LegendItem { mark: "up", tone: "flat", label: "Long entry" }
                LegendItem { mark: "down", tone: "flat", label: "Short entry" }
                LegendItem { mark: "dot", tone: "profit", label: "Exit in profit" }
                LegendItem { mark: "dot", tone: "loss", label: "Exit at a loss" }
            }
        }
    }
}

/// Entry marker, exit marker, and the line joining them.
#[component]
fn TradeMarks(
    overlay: TradeOverlay,
    entry_x: f64,
    entry_y: f64,
    exit: Option<(f64, f64)>,
) -> Element {
    let tone = Tone::of(overlay.profit_ratio);
    // A short's entry points down, as the legacy chart drew it.
    let triangle = if overlay.is_short {
        format!(
            "{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            entry_x,
            entry_y + 11.0,
            entry_x - 8.0,
            entry_y - 4.0,
            entry_x + 8.0,
            entry_y - 4.0
        )
    } else {
        format!(
            "{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            entry_x,
            entry_y - 11.0,
            entry_x - 8.0,
            entry_y + 4.0,
            entry_x + 8.0,
            entry_y + 4.0
        )
    };

    rsx! {
        g { class: "mark",
            if let Some((exit_x, exit_y)) = exit {
                line {
                    class: "mark__link mark__link--{tone.class()}",
                    x1: "{entry_x}", y1: "{entry_y}", x2: "{exit_x}", y2: "{exit_y}",
                    vector_effect: "non-scaling-stroke",
                }
                circle {
                    class: "mark__exit mark__exit--{tone.class()}",
                    cx: "{exit_x}", cy: "{exit_y}", r: "5.5",
                }
            }
            polygon {
                class: if overlay.is_open { "mark__entry mark__entry--open" } else { "mark__entry" },
                points: "{triangle}",
            }
        }
    }
}

#[component]
fn LegendItem(mark: &'static str, tone: &'static str, label: &'static str) -> Element {
    rsx! {
        span { class: "legend__item",
            span { class: "legend__mark legend__mark--{mark} legend__mark--{tone}" }
            span { "{label}" }
        }
    }
}
