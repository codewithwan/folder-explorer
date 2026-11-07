use crate::util::format::{Unit, format_size};
use crate::model::query::SummaryEntry;
use crate::ui::{SummaryRow, TreemapTile};
use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer};
use std::f32::consts::{FRAC_PI_2, TAU};

const CHART_SIZE: u32 = 220;
const MAX_SLICES: usize = 10;
const MAX_TREEMAP_TILES: usize = 12;

const PALETTE: [[u8; 3]; 10] = [
    [0x3b, 0x82, 0xf6], // Blue
    [0x10, 0xb9, 0x81], // Emerald
    [0xf5, 0x9e, 0x0b], // Amber
    [0x8b, 0x5c, 0xf6], // Violet
    [0xec, 0x48, 0x99], // Rose
    [0x06, 0xb6, 0xd4], // Cyan
    [0xf9, 0x73, 0x16], // Orange
    [0x63, 0x66, 0xf1], // Indigo
    [0x14, 0xb8, 0xa6], // Teal
    [0xea, 0xb3, 0x08], // Gold
];
const OTHER_RGB: [u8; 3] = [0x64, 0x74, 0x8b];

pub fn palette_rgb(index: usize) -> [u8; 3] {
    if index < PALETTE.len() { PALETTE[index] } else { OTHER_RGB }
}

pub fn palette_color(index: usize) -> Color {
    let [r, g, b] = palette_rgb(index);
    Color::from_rgb_u8(r, g, b)
}

pub fn format_percent(share: f32) -> String {
    let pct = share * 100.0;
    if pct <= 0.0 {
        "0%".into()
    } else if pct < 0.1 {
        "<0.1%".into()
    } else if pct >= 99.95 {
        "100%".into()
    } else {
        format!("{pct:.1}%")
    }
}

pub fn build_rows(entries: &[SummaryEntry], unit: Unit) -> (Vec<SummaryRow>, u64) {
    let total: u64 = entries.iter().map(|e| e.size).sum();
    let largest = entries.iter().map(|e| e.size).max().unwrap_or(0).max(1) as f32;
    let total_f = total.max(1) as f32;
    let rows = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let share = if total > 0 { entry.size as f32 / total_f } else { 0.0 };
            SummaryRow {
                label: entry.label.as_str().into(),
                detail: entry.detail(unit).into(),
                percent: format_percent(share).into(),
                ratio: entry.size as f32 / largest,
                share,
                color: palette_color(index),
            }
        })
        .collect();
    (rows, total)
}

pub fn build_treemap(entries: &[SummaryEntry], unit: Unit) -> Vec<TreemapTile> {
    let total: u64 = entries.iter().map(|e| e.size).sum();
    if total == 0 || entries.is_empty() {
        return Vec::new();
    }
    let total_f = total as f32;
    let limit = entries.len().min(MAX_TREEMAP_TILES);
    let mut items: Vec<(usize, String, String, String, f32, Color)> = entries
        .iter()
        .take(limit)
        .enumerate()
        .filter(|(_, e)| e.size > 0)
        .map(|(i, e)| {
            let share = e.size as f32 / total_f;
            (
                i,
                e.label.clone(),
                e.detail(unit),
                format_percent(share),
                e.size as f32,
                palette_color(i),
            )
        })
        .collect();

    if entries.len() > MAX_TREEMAP_TILES {
        let other_size: u64 = entries[MAX_TREEMAP_TILES..].iter().map(|e| e.size).sum();
        if other_size > 0 {
            let share = other_size as f32 / total_f;
            items.push((
                entries.len(),
                "Other".into(),
                format_size(other_size, unit),
                format_percent(share),
                other_size as f32,
                Color::from_rgb_u8(OTHER_RGB[0], OTHER_RGB[1], OTHER_RGB[2]),
            ));
        }
    }

    let mut tiles = Vec::with_capacity(items.len());
    layout_treemap(&items, 0.0, 0.0, 1.0, 1.0, &mut tiles);
    tiles
}

fn layout_treemap(
    items: &[(usize, String, String, String, f32, Color)],
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    out: &mut Vec<TreemapTile>,
) {
    if items.is_empty() || w <= 0.0 || h <= 0.0 {
        return;
    }
    if items.len() == 1 {
        let (index, label, detail, percent, _, color) = &items[0];
        out.push(TreemapTile {
            index: *index as i32,
            label: label.as_str().into(),
            detail: detail.as_str().into(),
            percent: percent.as_str().into(),
            x,
            y,
            w,
            h,
            color: *color,
        });
        return;
    }

    let sum: f32 = items.iter().map(|item| item.4).sum();
    if sum <= 0.0 {
        return;
    }

    let half = sum * 0.5;
    let mut acc = 0.0f32;
    let mut split = 1;
    for (i, item) in items.iter().enumerate() {
        acc += item.4;
        if acc >= half {
            split = (i + 1).clamp(1, items.len() - 1);
            break;
        }
    }

    let first_sum: f32 = items[..split].iter().map(|item| item.4).sum();
    let ratio = (first_sum / sum).clamp(0.08, 0.92);

    if w >= h {
        let w1 = w * ratio;
        layout_treemap(&items[..split], x, y, w1, h, out);
        layout_treemap(&items[split..], x + w1, y, w - w1, h, out);
    } else {
        let h1 = h * ratio;
        layout_treemap(&items[..split], x, y, w, h1, out);
        layout_treemap(&items[split..], x, y + h1, w, h - h1, out);
    }
}

mod wheel;
pub use wheel::*;

#[cfg(test)]
mod tests;
