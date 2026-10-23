//! Procedurally painted poster covers for the Films library — minimal movie
//! poster graphics (flat color fields, one bold geometric motif, big title
//! type), not book-cover gradients. Every script gets a cover generated from
//! a stable seed (its file path), so the same script always gets the same
//! cover; a custom image can replace it entirely (handled by the caller —
//! this module only draws the generated layer).
//!
//! The art is rasterized pixel-by-pixel (not built from GPUI divs) so shapes
//! get real anti-aliased edges and a little print grain.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::prelude::FluentBuilder;
use gpui::*;
use image::{Rgba as PixelRgba, RgbaImage};

// ---------------------------------------------------------------------------
// Deterministic randomness (ported from NEO's covers.js hash/rng)
// ---------------------------------------------------------------------------

fn hash_str(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed)
    }

    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64 / 4294967296.0) as f32
    }
}

fn between(r: &mut Rng, a: f32, b: f32) -> f32 {
    a + r.next_f32() * (b - a)
}

fn pick<'a, T>(r: &mut Rng, items: &'a [T]) -> &'a T {
    &items[(r.next_f32() * items.len() as f32) as usize % items.len()]
}

fn norm_hue(h: f32) -> f32 {
    ((h % 360.0) + 360.0) % 360.0 / 360.0
}

// ---------------------------------------------------------------------------
// Palette — two or three flat, bold colors (no soft gradient blending).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    bg: Hsla,
    shape: Hsla,
    shape2: Hsla,
}

fn make_palette(r: &mut Rng) -> Palette {
    let base = r.next_f32() * 360.0;
    let scheme = *pick(r, &["complement", "split", "triad", "near"]);
    let hue2 = match scheme {
        "complement" => base + 180.0,
        "split" => base + 150.0,
        "triad" => base + 120.0,
        _ => base + 35.0,
    };
    let dark = r.next_f32() < 0.55;
    let bg_sat = between(r, 45.0, 78.0);
    let bg_light = if dark { between(r, 12.0, 24.0) } else { between(r, 80.0, 93.0) };
    let shape_sat = between(r, 60.0, 90.0);
    let shape_light = if dark { between(r, 55.0, 72.0) } else { between(r, 28.0, 46.0) };
    let shape2_light = (shape_light + if dark { 15.0 } else { -15.0 }).clamp(8.0, 92.0);
    let col = |h: f32, s: f32, l: f32| hsla(norm_hue(h), (s / 100.0).clamp(0.0, 1.0), (l / 100.0).clamp(0.0, 1.0), 1.0);
    Palette {
        dark,
        bg: col(base, bg_sat, bg_light),
        shape: col(hue2, shape_sat, shape_light),
        shape2: col(hue2 + between(r, 20.0, 45.0), shape_sat * 0.85, shape2_light),
    }
}

// ---------------------------------------------------------------------------
// Plan — everything decided up front from the seed, so painting is pure.
// ---------------------------------------------------------------------------

pub struct CoverPlan {
    palette: Palette,
    style: &'static str,
    anchor: Anchor,
    frame: bool,
    rule: bool,
    seed: String,
}

#[derive(Clone, Copy)]
enum Anchor {
    Top,
    Center,
    Bottom,
}

const STYLES: [&str; 11] = [
    "halves", "horizon", "peaks", "bars", "eclipse", "arc", "grid", "slash",
    "constellation", "hatch", "waves",
];

/// Plan a cover from a stable seed (the script's file path, so renaming a
/// script doesn't reshuffle its art — only the title text changes).
pub fn plan_cover(seed: &str) -> CoverPlan {
    let mut r = Rng::new(hash_str(seed));
    let palette = make_palette(&mut r);
    let style = *pick(&mut r, &STYLES);

    let mut tr = Rng::new(hash_str(&format!("type:{seed}")));
    let anchor = *pick(&mut tr, &[Anchor::Top, Anchor::Center, Anchor::Bottom]);
    let frame = tr.next_f32() < 0.3;
    let rule = matches!(anchor, Anchor::Top) && tr.next_f32() < 0.4;

    CoverPlan { palette, style, anchor, frame, rule, seed: seed.to_string() }
}

// ---------------------------------------------------------------------------
// Raster painting — flat fields, crisp shapes, a little grain.
// ---------------------------------------------------------------------------

const CW: u32 = 240;
const CH: u32 = 320;

fn rgb8(c: Hsla) -> (f32, f32, f32) {
    let rgb = c.to_rgb();
    (rgb.r * 255.0, rgb.g * 255.0, rgb.b * 255.0)
}

fn lerp_rgb(a: Hsla, b: Hsla, t: f32) -> (u8, u8, u8) {
    let (ar, ag, ab) = rgb8(a);
    let (br, bg, bb) = rgb8(b);
    (
        (ar + (br - ar) * t).round().clamp(0.0, 255.0) as u8,
        (ag + (bg - ag) * t).round().clamp(0.0, 255.0) as u8,
        (ab + (bb - ab) * t).round().clamp(0.0, 255.0) as u8,
    )
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fill every pixel from a function of its position in 0..1 canvas space.
fn fill_with(buf: &mut RgbaImage, mut f: impl FnMut(f32, f32) -> (u8, u8, u8)) {
    let (w, h) = (buf.width() as f32, buf.height() as f32);
    for y in 0..buf.height() {
        for x in 0..buf.width() {
            let (r, g, b) = f(x as f32 / w, y as f32 / h);
            buf.put_pixel(x, y, PixelRgba([r, g, b, 255]));
        }
    }
}

fn fill_flat(buf: &mut RgbaImage, color: Hsla) {
    let rgb = lerp_rgb(color, color, 0.0);
    fill_with(buf, |_, _| rgb);
}

fn blend(buf: &mut RgbaImage, x: i32, y: i32, color: Hsla, alpha: f32) {
    if x < 0 || y < 0 || x as u32 >= buf.width() || y as u32 >= buf.height() {
        return;
    }
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.001 {
        return;
    }
    let (cr, cg, cb) = rgb8(color);
    let px = buf.get_pixel_mut(x as u32, y as u32);
    px[0] = (px[0] as f32 * (1.0 - a) + cr * a).round().clamp(0.0, 255.0) as u8;
    px[1] = (px[1] as f32 * (1.0 - a) + cg * a).round().clamp(0.0, 255.0) as u8;
    px[2] = (px[2] as f32 * (1.0 - a) + cb * a).round().clamp(0.0, 255.0) as u8;
    px[3] = 255;
}

/// A crisp, anti-aliased flat disc (1px edge feather only — no soft falloff).
fn paint_disc(buf: &mut RgbaImage, cx: f32, cy: f32, radius: f32, color: Hsla) {
    let x0 = (cx - radius - 1.0).floor().max(0.0) as i32;
    let x1 = (cx + radius + 1.0).ceil().min(buf.width() as f32) as i32;
    let y0 = (cy - radius - 1.0).floor().max(0.0) as i32;
    let y1 = (cy + radius + 1.0).ceil().min(buf.height() as f32) as i32;
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let edge = ((radius - dist) + 1.0).clamp(0.0, 2.0) / 2.0;
            if edge > 0.0 {
                blend(buf, x, y, color, edge);
            }
        }
    }
}

fn paint_rect(buf: &mut RgbaImage, x0: f32, y0: f32, x1: f32, y1: f32, color: Hsla) {
    let xi0 = x0.floor().max(0.0) as i32;
    let xi1 = x1.ceil().min(buf.width() as f32) as i32;
    let yi0 = y0.floor().max(0.0) as i32;
    let yi1 = y1.ceil().min(buf.height() as f32) as i32;
    for y in yi0..yi1 {
        for x in xi0..xi1 {
            blend(buf, x, y, color, 1.0);
        }
    }
}

/// A straight diagonal band of a given thickness (as a fraction of the
/// canvas's shorter side), anti-aliased — for bold Saul-Bass-style slashes.
fn paint_diagonal_band(buf: &mut RgbaImage, slope: f32, center: f32, thickness: f32, color: Hsla) {
    let (w, h) = (buf.width() as f32, buf.height() as f32);
    for y in 0..buf.height() {
        for x in 0..buf.width() {
            let u = x as f32 / w - 0.5;
            let v = y as f32 / h - 0.5;
            let d = (v - u * slope - center).abs();
            let a = 1.0 - smoothstep(thickness * 0.5 - 0.006, thickness * 0.5 + 0.006, d);
            if a > 0.0 {
                blend(buf, x as i32, y as i32, color, a);
            }
        }
    }
}

/// A thick, anti-aliased line segment — the primitive behind the
/// constellation, hatch and wave styles (real line art, not filled shapes).
fn draw_line(buf: &mut RgbaImage, x0: f32, y0: f32, x1: f32, y1: f32, color: Hsla, thickness: f32, alpha: f32) {
    let (w, h) = (buf.width() as f32, buf.height() as f32);
    let pad = thickness + 1.0;
    let min_x = (x0.min(x1) - pad).floor().max(0.0) as i32;
    let max_x = (x0.max(x1) + pad).ceil().min(w) as i32;
    let min_y = (y0.min(y1) - pad).floor().max(0.0) as i32;
    let max_y = (y0.max(y1) + pad).ceil().min(h) as i32;
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len2 = (dx * dx + dy * dy).max(0.0001);
    for y in min_y..max_y {
        for x in min_x..max_x {
            let px = x as f32 + 0.5 - x0;
            let py = y as f32 + 0.5 - y0;
            let t = ((px * dx + py * dy) / len2).clamp(0.0, 1.0);
            let (cx, cy) = (x0 + dx * t, y0 + dy * t);
            let dist = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let a = (1.0 - smoothstep(thickness * 0.5 - 0.6, thickness * 0.5 + 0.6, dist)) * alpha;
            if a > 0.0 {
                blend(buf, x, y, color, a);
            }
        }
    }
}

/// A circle filled with parallel line hatching instead of a flat color —
/// the engraved, line-textured look from pxl-pshr's geometric pattern generator.
fn paint_hatched_disc(buf: &mut RgbaImage, cx: f32, cy: f32, radius: f32, angle_deg: f32, spacing: f32, color: Hsla) {
    let theta = angle_deg.to_radians();
    let (dx, dy) = (theta.cos(), theta.sin());
    let (nx, ny) = (-dy, dx);
    let steps = (radius * 2.0 / spacing) as i32 + 2;
    for i in -steps..=steps {
        let offset = i as f32 * spacing;
        if offset.abs() >= radius {
            continue;
        }
        let half_len = (radius * radius - offset * offset).max(0.0).sqrt();
        let (lx, ly) = (cx + nx * offset, cy + ny * offset);
        let (x0, y0) = (lx - dx * half_len, ly - dy * half_len);
        let (x1, y1) = (lx + dx * half_len, ly + dy * half_len);
        draw_line(buf, x0, y0, x1, y1, color, 1.3, 0.9);
    }
}

fn paint_grain(buf: &mut RgbaImage, r: &mut Rng, amount: f32) {
    for px in buf.pixels_mut() {
        let n = (r.next_f32() - 0.5) * amount;
        for c in px.0.iter_mut().take(3) {
            *c = (*c as f32 + n).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// A continuous jagged skyline, filled flat below it — the silhouette
/// mountains a lot of adventure/western posters lean on.
fn paint_peaks(buf: &mut RgbaImage, r: &mut Rng, p: &Palette) {
    let n = 3 + (r.next_f32() * 2.0) as usize;
    let base_y = between(r, 0.64, 0.8);
    let mut xs = vec![0.0f32];
    let mut ys = vec![base_y];
    for i in 0..n {
        let seg = 1.0 / n as f32;
        xs.push(seg * (i as f32 + between(r, 0.3, 0.7)));
        ys.push(base_y - between(r, 0.14, 0.36));
    }
    xs.push(1.0);
    ys.push(base_y);

    let h = buf.height() as f32;
    for px_ in 0..buf.width() {
        let u = px_ as f32 / buf.width() as f32;
        let mut seg_i = xs.len() - 2;
        for i in 0..xs.len() - 1 {
            if u >= xs[i] && u <= xs[i + 1] {
                seg_i = i;
                break;
            }
        }
        let (x0, x1, y0v, y1v) = (xs[seg_i], xs[seg_i + 1], ys[seg_i], ys[seg_i + 1]);
        let t = if x1 > x0 { (u - x0) / (x1 - x0) } else { 0.0 };
        let sky_y = ((y0v + (y1v - y0v) * t) * h).round() as i32;
        for py_ in sky_y.max(0)..buf.height() as i32 {
            blend(buf, px_ as i32, py_, p.shape, 1.0);
        }
    }
}

fn paint_style(buf: &mut RgbaImage, plan: &CoverPlan, r: &mut Rng) {
    let p = &plan.palette;
    let (w, h) = (CW as f32, CH as f32);

    match plan.style {
        "halves" => {
            let slope = between(r, -0.5, 0.5);
            let offset = between(r, -0.1, 0.1);
            let flip = r.next_f32() < 0.5;
            let (br, bgc, bb) = rgb8(p.bg);
            let (sr, sg, sb) = rgb8(p.shape);
            fill_with(buf, |u, v| {
                let d = (v - 0.5) - (u - 0.5) * slope - offset;
                let d = if flip { -d } else { d };
                let t = smoothstep(-0.006, 0.006, d);
                (
                    (br + (sr - br) * t).round() as u8,
                    (bgc + (sg - bgc) * t).round() as u8,
                    (bb + (sb - bb) * t).round() as u8,
                )
            });
        }
        "horizon" => {
            fill_flat(buf, p.bg);
            let y = between(r, 0.5, 0.72) * h;
            paint_rect(buf, 0.0, y, w, h, p.shape);
            if r.next_f32() < 0.45 {
                let sun_r = between(r, 0.14, 0.22) * w;
                let cx = between(r, 0.3, 0.7) * w;
                let cy = y - between(r, 0.1, 0.55) * sun_r;
                paint_disc(buf, cx, cy, sun_r, p.shape2);
            }
        }
        "peaks" => {
            fill_flat(buf, p.bg);
            paint_peaks(buf, r, p);
        }
        "bars" => {
            fill_flat(buf, p.bg);
            let vertical = r.next_f32() < 0.5;
            let total = if vertical { w } else { h };
            let n = 2 + (r.next_f32() * 2.0) as i32;
            let mut pos = between(r, 0.0, 0.15) * total;
            let mut toggle = r.next_f32() < 0.5;
            for _ in 0..n {
                if pos > total {
                    break;
                }
                let bar_w = between(r, 0.18, 0.3) * total;
                let color = if toggle { p.shape } else { p.shape2 };
                if vertical {
                    paint_rect(buf, pos, 0.0, pos + bar_w, h, color);
                } else {
                    paint_rect(buf, 0.0, pos, w, pos + bar_w, color);
                }
                pos += bar_w + between(r, 0.05, 0.14) * total;
                toggle = !toggle;
            }
        }
        "eclipse" => {
            // A target/bullseye motif — off-center and 2 or 3 rings, so it
            // doesn't read as the same medallion every time.
            fill_flat(buf, p.bg);
            let cx = between(r, 0.22, 0.78) * w;
            let cy = between(r, 0.2, 0.48) * h;
            let radius = between(r, 0.26, 0.4) * w;
            paint_disc(buf, cx, cy, radius, p.shape);
            if r.next_f32() < 0.6 {
                paint_disc(buf, cx, cy, radius * 0.6, p.bg);
                paint_disc(buf, cx, cy, radius * 0.3, p.shape2);
            } else {
                paint_disc(buf, cx, cy, radius * 0.45, p.shape2);
            }
        }
        "arc" => {
            // A big disc bleeding off the top or bottom edge — a
            // rising/setting-sun motif, distinct from a centered medallion.
            fill_flat(buf, p.bg);
            let at_bottom = r.next_f32() < 0.5;
            let cx = between(r, 0.25, 0.75) * w;
            let radius = between(r, 0.55, 0.85) * w;
            let cy = if at_bottom { h + between(r, 0.0, 0.1) * h } else { -between(r, 0.0, 0.1) * h };
            paint_disc(buf, cx, cy, radius, p.shape);
        }
        "grid" => {
            // A sparse dot grid — a halftone-poster texture, not a solid shape.
            fill_flat(buf, p.bg);
            let cols = 4 + (r.next_f32() * 3.0) as i32;
            let rows = 6 + (r.next_f32() * 4.0) as i32;
            let dot_r = (w / cols as f32) * between(r, 0.2, 0.3);
            for row in 0..rows {
                for col in 0..cols {
                    if r.next_f32() < 0.55 {
                        let cx = (col as f32 + 0.5) / cols as f32 * w;
                        let cy = (row as f32 + 0.5) / rows as f32 * h;
                        let color = if r.next_f32() < 0.75 { p.shape } else { p.shape2 };
                        paint_disc(buf, cx, cy, dot_r, color);
                    }
                }
            }
        }
        "slash" => {
            // Two or three thick diagonal bars crossing the whole poster.
            fill_flat(buf, p.bg);
            let slope = between(r, 0.5, 1.3) * if r.next_f32() < 0.5 { 1.0 } else { -1.0 };
            let n = 2 + (r.next_f32() * 2.0) as i32;
            for i in 0..n {
                let center = between(r, -0.5, 0.5);
                let thickness = between(r, 0.14, 0.24);
                let color = if i % 2 == 0 { p.shape } else { p.shape2 };
                paint_diagonal_band(buf, slope, center, thickness, color);
            }
        }
        "constellation" => {
            // Scattered points joined by thin lines — a star-map / web of
            // connections, drawn as real line art rather than filled shapes.
            fill_flat(buf, p.bg);
            let n = 9 + (r.next_f32() * 6.0) as usize;
            let mut pts = Vec::with_capacity(n);
            for _ in 0..n {
                pts.push((between(r, 0.08, 0.92) * w, between(r, 0.08, 0.88) * h));
            }
            for i in 0..pts.len() {
                let j = (i + 1) % pts.len();
                if r.next_f32() < 0.8 {
                    draw_line(buf, pts[i].0, pts[i].1, pts[j].0, pts[j].1, p.shape2, 1.3, 0.8);
                }
                if r.next_f32() < 0.35 {
                    let k = (r.next_f32() * pts.len() as f32) as usize % pts.len();
                    draw_line(buf, pts[i].0, pts[i].1, pts[k].0, pts[k].1, p.shape2, 1.0, 0.5);
                }
            }
            for (px_, py_) in &pts {
                let dot_r = between(r, 2.0, 5.0);
                paint_disc(buf, *px_, *py_, dot_r, p.shape);
            }
        }
        "hatch" => {
            // Circles filled with parallel line hatching — an engraved,
            // line-textured look rather than a flat fill.
            fill_flat(buf, p.bg);
            let n_shapes = 1 + (r.next_f32() * 2.0) as i32;
            for i in 0..n_shapes {
                let cx = between(r, 0.25, 0.75) * w;
                let cy = between(r, 0.2, 0.55) * h;
                let radius = between(r, 0.22, 0.38) * w;
                let angle = between(r, 0.0, 180.0);
                let spacing = between(r, 4.0, 8.0);
                let color = if i == 0 { p.shape } else { p.shape2 };
                paint_hatched_disc(buf, cx, cy, radius, angle, spacing, color);
            }
        }
        _ => {
            // "waves" — stacked sine lines, like a signal or a topographic map.
            fill_flat(buf, p.bg);
            let rows = 5 + (r.next_f32() * 6.0) as i32;
            let amp = between(r, 0.02, 0.05) * h;
            let freq = between(r, 2.0, 5.0);
            let phase = between(r, 0.0, std::f32::consts::TAU);
            for i in 0..rows {
                let base_y = (i as f32 + 1.0) / (rows as f32 + 1.0) * h;
                let color = if i % 2 == 0 { p.shape } else { p.shape2 };
                let steps = 48;
                let mut prev: Option<(f32, f32)> = None;
                for s in 0..=steps {
                    let x = s as f32 / steps as f32 * w;
                    let y = base_y + (x / w * std::f32::consts::TAU * freq + phase).sin() * amp;
                    if let Some((px_, py_)) = prev {
                        draw_line(buf, px_, py_, x, y, color, 1.6, 0.9);
                    }
                    prev = Some((x, y));
                }
            }
        }
    }

    let grain_amount = between(r, 3.0, 6.0);
    paint_grain(buf, r, grain_amount);
}

fn render_bitmap(plan: &CoverPlan) -> RgbaImage {
    let mut buf = RgbaImage::new(CW, CH);
    let mut r = Rng::new(hash_str(&format!("paint:{}", plan.seed)));
    paint_style(&mut buf, plan, &mut r);
    buf
}

fn image_cache() -> &'static Mutex<HashMap<String, Arc<RenderImage>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<RenderImage>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The painted art for a plan, as a GPUI-displayable bitmap. Cached by seed
/// so scrolling the library doesn't repaint every frame.
fn painted_image(plan: &CoverPlan) -> Arc<RenderImage> {
    let mut cache = image_cache().lock().unwrap();
    if let Some(img) = cache.get(&plan.seed) {
        return img.clone();
    }
    let mut buf = render_bitmap(plan);
    // RenderImage expects BGRA; `image`'s buffers are RGBA.
    for px in buf.pixels_mut() {
        px.0.swap(0, 2);
    }
    let image = Arc::new(RenderImage::new(vec![image::Frame::new(buf)]));
    cache.insert(plan.seed.clone(), image.clone());
    image
}

// ---------------------------------------------------------------------------
// Title overlay
// ---------------------------------------------------------------------------

/// Compose the generated art with the title type — the dominant element on a
/// real poster — over a flat scrim bar rather than a soft gradient fade.
pub fn render_cover(plan: &CoverPlan, title: &str) -> impl IntoElement {
    let ink = if plan.palette.dark { hsla(0.0, 0.0, 0.97, 1.0) } else { hsla(0.0, 0.0, 0.08, 1.0) };
    let scrim = if plan.palette.dark { hsla(0.0, 0.0, 0.0, 0.55) } else { hsla(0.0, 0.0, 1.0, 0.55) };

    let title_block = div()
        .absolute()
        .left(px(10.0))
        .right(px(10.0))
        .when(matches!(plan.anchor, Anchor::Top), |d| d.top(px(14.0)))
        .when(matches!(plan.anchor, Anchor::Center), |d| d.top(relative(0.4)))
        .when(matches!(plan.anchor, Anchor::Bottom), |d| d.bottom(px(14.0)))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .when(plan.rule, |d| {
            d.child(div().w(px(28.0)).h(px(3.0)).bg(plan.palette.shape2).mb(px(6.0)))
        })
        .child(
            div()
                .text_size(px(18.0))
                .line_height(px(21.0))
                .font_weight(FontWeight::BOLD)
                .text_color(ink)
                .when(matches!(plan.anchor, Anchor::Center), |d| d.text_center())
                .child(title.to_uppercase()),
        );

    div()
        .relative()
        .size_full()
        .child(img(painted_image(plan)).size_full().object_fit(ObjectFit::Cover))
        .when(plan.frame, |d| {
            d.child(
                div().absolute().inset(px(6.0)).border_1().border_color(hsla(ink.h, ink.s, ink.l, 0.5)),
            )
        })
        .child(
            // Flat scrim bar behind the title, matching a poster's solid
            // title-block treatment rather than a soft photographic fade.
            match plan.anchor {
                Anchor::Top => div().absolute().left_0().top_0().w_full().h(px(52.0)).bg(scrim),
                Anchor::Bottom => div().absolute().left_0().bottom_0().w_full().h(px(52.0)).bg(scrim),
                Anchor::Center => div().absolute().left_0().top(relative(0.38)).w_full().h(px(48.0)).bg(scrim),
            },
        )
        .child(title_block)
}

/// Stable per-script seed used for art (not for the title text), so renaming
/// a script doesn't reshuffle its cover. `variant` is bumped by "New Random
/// Cover" in the library's right-click menu to pick a different look.
pub fn seed_for_path(path: &std::path::Path, variant: u32) -> String {
    let base = path.to_string_lossy();
    if variant == 0 {
        base.to_string()
    } else {
        format!("{base}#{variant}")
    }
}
