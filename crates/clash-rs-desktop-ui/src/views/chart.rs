//! Live traffic chart for the Overview page.
//!
//! The framework only fills with solid colors (no gradients), so this draws two
//! stroked polylines — upload and download — over a dark panel, built from
//! positioned `Shape::line` segments on a `Canvas`. A bounded ring buffer of the
//! most recent `/ws/traffic` samples (fed by the Overview subscription) backs it.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use windows_reactor::*;

use crate::views::design::{HAIRLINE, palette, radius, spacing, type_scale};

/// How many recent samples the chart shows (one per traffic tick, ~1/s).
const CHART_CAPACITY: usize = 60;
const CHART_HEIGHT: f64 = 150.0;

static TRAFFIC_HISTORY: OnceLock<Mutex<VecDeque<(u64, u64)>>> = OnceLock::new();

fn history() -> &'static Mutex<VecDeque<(u64, u64)>> {
    TRAFFIC_HISTORY.get_or_init(|| Mutex::new(VecDeque::with_capacity(CHART_CAPACITY)))
}

/// Append one `(up, down)` sample, evicting the oldest beyond the capacity.
pub(crate) fn push_traffic_sample(up: u64, down: u64) {
    if let Ok(mut buf) = history().lock() {
        if buf.len() == CHART_CAPACITY {
            buf.pop_front();
        }
        buf.push_back((up, down));
    }
}

/// Snapshot the current samples (oldest → newest).
pub(crate) fn traffic_samples() -> Vec<(u64, u64)> {
    history()
        .lock()
        .map(|buf| buf.iter().copied().collect())
        .unwrap_or_default()
}

fn format_rate(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    if bytes >= MIB {
        format!("{:.2} MiB/s", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB/s", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B/s")
    }
}

/// Map `samples` (one channel) to (x, y) pixel coordinates within `width`×`height`,
/// where y is inverted (0 = top) and scaled to `max`. Public for unit testing.
pub(crate) fn series_points(
    values: &[u64],
    width: f64,
    height: f64,
    max: u64,
) -> Vec<(f64, f64)> {
    if values.is_empty() {
        return Vec::new();
    }
    let max = max.max(1) as f64;
    let step = if values.len() > 1 {
        width / (values.len() - 1) as f64
    } else {
        0.0
    };
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = i as f64 * step;
            let y = height - (v as f64 / max * height);
            (x, y.clamp(0.0, height))
        })
        .collect()
}

fn polyline(points: &[(f64, f64)], color: Color) -> Vec<Element> {
    points
        .windows(2)
        .map(|seg| {
            let (x1, y1) = seg[0];
            let (x2, y2) = seg[1];
            Shape::line(x1, y1, x2, y2)
                .stroke(color)
                .stroke_thickness(2.0)
                .into()
        })
        .collect()
}

/// The live traffic chart card: a dark panel with a legend showing the current
/// up/down rates and the two polylines plotted beneath.
pub(crate) fn traffic_chart(samples: &[(u64, u64)], width: f64) -> Element {
    let (cur_up, cur_down) = samples.last().copied().unwrap_or((0, 0));
    let ups: Vec<u64> = samples.iter().map(|s| s.0).collect();
    let downs: Vec<u64> = samples.iter().map(|s| s.1).collect();
    let max = ups
        .iter()
        .chain(downs.iter())
        .copied()
        .max()
        .unwrap_or(0);

    let plot_w = (width - spacing::XL * 2.0).max(40.0);
    let up_pts = series_points(&ups, plot_w, CHART_HEIGHT, max);
    let down_pts = series_points(&downs, plot_w, CHART_HEIGHT, max);

    let mut shapes: Vec<Element> = Vec::new();
    shapes.extend(polyline(&down_pts, palette::CHART_DOWN));
    shapes.extend(polyline(&up_pts, palette::CHART_UP));

    let plot: Element = if samples.len() < 2 {
        // Not enough points to draw a line yet.
        border(
            text_block("Collecting traffic…")
                .font_size(type_scale::CAPTION)
                .foreground(tokens::TertiaryText),
        )
        .height(CHART_HEIGHT)
        .into()
    } else {
        Canvas::new(shapes).height(CHART_HEIGHT).into()
    };

    let legend = hstack((
        legend_item(palette::CHART_UP, "Upload", &format_rate(cur_up)),
        legend_item(palette::CHART_DOWN, "Download", &format_rate(cur_down)),
    ))
    .spacing(spacing::XL);

    border(
        vstack((
            hstack((
                text_block("Network traffic")
                    .font_size(type_scale::BODY_STRONG)
                    .semibold()
                    .foreground(Color::rgb(0xF5, 0xF5, 0xF7)),
                legend
                    .horizontal_alignment(HorizontalAlignment::Right),
            ))
            .grid_like(),
            plot,
        ))
        .spacing(spacing::MD),
    )
    .padding(Thickness::uniform(spacing::XL))
    .corner_radius(radius::LG)
    .background(palette::CHART_PANEL)
    .border_thickness(Thickness::uniform(HAIRLINE))
    .border_brush(Color::rgb(0x2A, 0x33, 0x40))
    .into()
}

fn legend_item(color: Color, label: &str, value: &str) -> Element {
    hstack((
        Shape::ellipse().fill(color).width(8.0).height(8.0),
        text_block(label.to_owned())
            .font_size(type_scale::CAPTION)
            .foreground(Color::rgb(0xB0, 0xB8, 0xC4)),
        text_block(value.to_owned())
            .font_size(type_scale::CAPTION)
            .semibold()
            .foreground(color),
    ))
    .spacing(spacing::SM)
    .into()
}

/// Small helper: a row whose first child takes remaining space and the rest sit
/// to the right. (StackPanel has no justify-between, so widen via alignment.)
trait GridLike {
    fn grid_like(self) -> Element;
}

impl GridLike for StackPanel {
    fn grid_like(self) -> Element {
        self.horizontal_alignment(HorizontalAlignment::Stretch).into()
    }
}

#[cfg(test)]
mod tests {
    use super::series_points;

    #[test]
    fn series_points_inverts_y_and_scales_to_max() {
        // Two samples, max 100, height 100, width 100.
        let pts = series_points(&[0, 100], 100.0, 100.0, 100);
        assert_eq!(pts.len(), 2);
        // First sample value 0 → bottom (y = height).
        assert_eq!(pts[0], (0.0, 100.0));
        // Second sample value 100 (== max) → top (y = 0), x at full width.
        assert_eq!(pts[1], (100.0, 0.0));
    }

    #[test]
    fn series_points_empty_is_empty() {
        assert!(series_points(&[], 100.0, 100.0, 10).is_empty());
    }

    #[test]
    fn series_points_handles_zero_max() {
        // All-zero samples must not divide by zero; everything sits on the floor.
        let pts = series_points(&[0, 0, 0], 90.0, 100.0, 0);
        assert_eq!(pts.len(), 3);
        assert!(pts.iter().all(|&(_, y)| y == 100.0));
    }
}
