use super::*;

struct SliceSpan {
    index: usize,
    start: f32,
    end: f32,
    rgb: [u8; 3],
}

fn compute_spans(entries: &[SummaryEntry]) -> Vec<SliceSpan> {
    let total: u64 = entries.iter().map(|e| e.size).sum();
    if total == 0 || entries.is_empty() {
        return Vec::new();
    }
    let total_f = total as f32;
    let mut spans = Vec::new();
    let mut cursor = 0.0f32;
    let limit = entries.len().min(MAX_SLICES);
    for (i, entry) in entries.iter().take(limit).enumerate() {
        if entry.size == 0 {
            continue;
        }
        let is_last = i + 1 == limit && entries.len() <= MAX_SLICES;
        let delta = (entry.size as f32 / total_f) * TAU;
        let end = if is_last { TAU } else { (cursor + delta).min(TAU) };
        if end > cursor + 0.004 {
            spans.push(SliceSpan { index: i, start: cursor, end, rgb: palette_rgb(i) });
            cursor = end;
        }
    }
    if entries.len() > MAX_SLICES && cursor < TAU - 0.004 {
        spans.push(SliceSpan {
            index: MAX_SLICES,
            start: cursor,
            end: TAU,
            rgb: OTHER_RGB,
        });
    } else if let Some(last) = spans.last_mut() {
        last.end = TAU;
    }
    spans
}

pub fn hit_test_wheel(entries: &[SummaryEntry], donut: bool, nx: f32, ny: f32) -> Option<usize> {
    if nx.abs() > 1.1 || ny.abs() > 1.1 {
        return None;
    }
    let r = (nx * nx + ny * ny).sqrt();
    let r_min = if donut { 48.0 / 110.0 } else { 0.0 };
    let r_max = 102.0 / 110.0;
    if r < r_min || r > r_max {
        return None;
    }
    let spans = compute_spans(entries);
    if spans.is_empty() {
        return None;
    }
    let angle = (ny.atan2(nx) + FRAC_PI_2).rem_euclid(TAU);
    spans
        .iter()
        .find(|span| angle >= span.start && angle <= span.end)
        .map(|span| span.index)
        .filter(|index| *index < MAX_SLICES)
}

pub fn render_wheel(entries: &[SummaryEntry], donut: bool, hovered: Option<usize>) -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(CHART_SIZE, CHART_SIZE);
    let spans = compute_spans(entries);
    if spans.is_empty() {
        return Image::from_rgba8(buffer);
    }

    let hovered_slice = hovered.map(|idx| idx.min(MAX_SLICES));
    let multi = spans.len() > 1;
    let center = CHART_SIZE as f32 * 0.5;
    let pixels = buffer.make_mut_slice();

    for py in 0..CHART_SIZE {
        let dy0 = py as f32 + 0.5 - center;
        for px in 0..CHART_SIZE {
            let dx0 = px as f32 + 0.5 - center;
            let angle0 = (dy0.atan2(dx0) + FRAC_PI_2).rem_euclid(TAU);

            let Some(span) = spans.iter().find(|s| angle0 >= s.start && angle0 <= s.end) else {
                continue;
            };

            let is_hovered = hovered_slice == Some(span.index);
            let mid = 0.5 * (span.start + span.end);
            let (ox, oy) = if is_hovered && !donut {
                let shift = 5.0;
                (mid.sin() * shift, -mid.cos() * shift)
            } else {
                (0.0, 0.0)
            };

            let dx = dx0 - ox;
            let dy = dy0 - oy;
            let r = (dx * dx + dy * dy).sqrt();

            let r_outer = if is_hovered {
                if donut { 101.5 } else { 97.5 }
            } else {
                94.5
            };
            let r_inner = if donut {
                if is_hovered { 50.5 } else { 53.0 }
            } else {
                0.0
            };

            let outer_aa = (r_outer + 0.5 - r).clamp(0.0, 1.0);
            let inner_aa = if donut { (r - (r_inner - 0.5)).clamp(0.0, 1.0) } else { 1.0 };
            let mut alpha = outer_aa * inner_aa;
            if alpha <= 0.0 {
                continue;
            }

            if multi && r > 0.5 {
                let angle = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU);
                let d_start = (angle - span.start).abs().min(TAU - (angle - span.start).abs()) * r;
                let d_end = (span.end - angle).abs().min(TAU - (span.end - angle).abs()) * r;
                let gap_dist = d_start.min(d_end);
                let half_gap = 0.85;
                let gap_aa = (gap_dist - half_gap + 0.5).clamp(0.0, 1.0);
                alpha *= gap_aa;
            }

            if alpha <= 0.0 {
                continue;
            }

            let mut rgb = span.rgb;
            if is_hovered {
                rgb = [
                    rgb[0].saturating_add(24),
                    rgb[1].saturating_add(24),
                    rgb[2].saturating_add(24),
                ];
            } else if hovered_slice.is_some() {
                alpha *= 0.78;
            }

            // Subtle radial sheen for depth
            let t = ((r - r_inner) / (r_outer - r_inner).max(1.0)).clamp(0.0, 1.0);
            let sheen = (1.0 - t) * 14.0;
            let r_u8 = (rgb[0] as f32 + sheen).clamp(0.0, 255.0) as u8;
            let g_u8 = (rgb[1] as f32 + sheen).clamp(0.0, 255.0) as u8;
            let b_u8 = (rgb[2] as f32 + sheen).clamp(0.0, 255.0) as u8;
            let a_u8 = (alpha * 255.0).round() as u8;

            pixels[(py * CHART_SIZE + px) as usize] = Rgba8Pixel { r: r_u8, g: g_u8, b: b_u8, a: a_u8 };
        }
    }

    Image::from_rgba8(buffer)
}
