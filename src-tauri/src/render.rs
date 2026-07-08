//! The template executor (PRD §5): one layout model, defined in CSS units from
//! the design fixture, renders the static sheet, the animated grid frames and
//! the header — so every artifact derives from the same template spec.

use crate::theme;
use crate::types::{GridDims, OrientationMode, VideoMeta};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgb, RgbImage};

// ---------------------------------------------------------------- fonts

pub struct Fonts {
    pub medium: FontRef<'static>,
    pub semibold: FontRef<'static>,
    pub bold: FontRef<'static>,
}

impl Fonts {
    pub fn load() -> Self {
        Fonts {
            medium: FontRef::try_from_slice(theme::FONT_MEDIUM).expect("bundled font"),
            semibold: FontRef::try_from_slice(theme::FONT_SEMIBOLD).expect("bundled font"),
            bold: FontRef::try_from_slice(theme::FONT_BOLD).expect("bundled font"),
        }
    }
}

fn blend(dst: &mut Rgb<u8>, src: Rgb<u8>, a: f32) {
    let a = a.clamp(0.0, 1.0);
    for i in 0..3 {
        dst.0[i] = (dst.0[i] as f32 * (1.0 - a) + src.0[i] as f32 * a).round() as u8;
    }
}

/// Draw `text` with its top-left at (x, y). Returns the advance width.
/// `tracking` is extra letter-spacing in px (the 0.06em of the label style).
#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    img: &mut RgbImage,
    font: &FontRef,
    px: f32,
    color: Rgb<u8>,
    x: f32,
    y: f32,
    tracking: f32,
    text: &str,
) -> f32 {
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let mut caret = x;
    for c in text.chars() {
        let gid = scaled.glyph_id(c);
        let glyph = gid.with_scale_and_position(scale, ab_glyph::point(caret, y + ascent));
        if let Some(og) = font.outline_glyph(glyph) {
            let b = og.px_bounds();
            og.draw(|gx, gy, cov| {
                let px_x = b.min.x as i32 + gx as i32;
                let px_y = b.min.y as i32 + gy as i32;
                if px_x >= 0 && px_y >= 0 && px_x < iw && px_y < ih && cov > 0.01 {
                    blend(img.get_pixel_mut(px_x as u32, px_y as u32), color, cov);
                }
            });
        }
        caret += scaled.h_advance(gid) + tracking;
    }
    caret - x
}

pub fn measure_text(font: &FontRef, px: f32, tracking: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(px));
    text.chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)) + tracking)
        .sum()
}

/// Truncate with ellipsis to fit `max_w` (PRD FR11: never overflow the band).
pub fn ellipsize(font: &FontRef, px: f32, text: &str, max_w: f32) -> String {
    if measure_text(font, px, 0.0, text) <= max_w {
        return text.to_string();
    }
    let mut s: Vec<char> = text.chars().collect();
    while !s.is_empty() {
        s.pop();
        let cand: String = s.iter().collect::<String>() + "…";
        if measure_text(font, px, 0.0, &cand) <= max_w {
            return cand;
        }
    }
    "…".into()
}

// ---------------------------------------------------------------- layout

/// CSS-unit constants from the design fixture (fixture.html).
const CSS_CARD_W: f64 = 900.0;
const CSS_PAD_X: f64 = 32.0;
const CSS_PAD_Y: f64 = 28.0;
const CSS_GAP: f64 = 8.0;
const CSS_TILE_RADIUS: f64 = 3.0;
const CSS_HEADER_BLOCK_H: f64 = 30.0; // title / label+value block height
const CSS_HEADER_PAD_B: f64 = 14.0; // padding above the divider
const CSS_HEADER_MARGIN_B: f64 = 20.0; // divider → grid

#[derive(Debug, Clone)]
pub struct SheetLayout {
    pub scale: f64, // px per CSS unit
    pub cols: u32,
    pub rows: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub gap: u32,
    pub pad_x: u32,
    pub pad_y: u32,
    pub grid_top: u32, // y where the tile grid starts
    pub card_w: u32,
    pub card_h: u32,
    pub hairline: u32,
}

/// Tile aspect ratio per the orientation policy (PRD FR8a): Auto follows the
/// source (fit, never crop); explicit modes force 16:9 / 9:16 and letterbox.
pub fn tile_aspect(mode: OrientationMode, meta: &VideoMeta) -> f64 {
    match mode {
        OrientationMode::Auto => meta.aspect().clamp(0.42, 2.4),
        OrientationMode::Portrait => 9.0 / 16.0,
        OrientationMode::Landscape => 16.0 / 9.0,
    }
}

/// Build a layout at a given render scale (px per CSS unit).
pub fn layout(grid: GridDims, aspect: f64, scale: f64) -> SheetLayout {
    let cols = grid.cols.max(1);
    let rows = grid.rows.max(1);
    let tile_w_css = (CSS_CARD_W - 2.0 * CSS_PAD_X - (cols as f64 - 1.0) * CSS_GAP) / cols as f64;
    let tile_h_css = tile_w_css / aspect;

    let s = scale;
    let tile_w = (tile_w_css * s).round().max(16.0) as u32;
    let tile_h = (tile_h_css * s).round().max(16.0) as u32;
    let gap = (CSS_GAP * s).round().max(1.0) as u32;
    let pad_x = (CSS_PAD_X * s).round() as u32;
    let pad_y = (CSS_PAD_Y * s).round() as u32;
    let hairline = (s.round().max(1.0)) as u32;

    let header_h = ((CSS_HEADER_BLOCK_H + CSS_HEADER_PAD_B + CSS_HEADER_MARGIN_B) * s).round()
        as u32
        + hairline;
    let grid_top = pad_y + header_h;
    let card_w = 2 * pad_x + cols * tile_w + (cols - 1) * gap;
    let card_h = grid_top + rows * tile_h + (rows - 1) * gap + pad_y;

    SheetLayout {
        scale: s,
        cols,
        rows,
        tile_w,
        tile_h,
        gap,
        pad_x,
        pad_y,
        grid_top,
        card_w,
        card_h,
        hairline,
    }
}

/// Layout for the static sheet: fixed 2× device scale (PRD FR20).
pub fn static_layout(grid: GridDims, aspect: f64, scale_mult: f64) -> SheetLayout {
    layout(grid, aspect, 2.0 * scale_mult)
}

/// Layout for the animated grid: scale chosen so the tile's long side hits the
/// quality-mapped pixel budget (proven ~428px safe → ~640px crisp, PRD FR13).
pub fn animated_layout(grid: GridDims, aspect: f64, quality: u8, scale_mult: f64) -> SheetLayout {
    let long_side = (280.0 + 3.6 * quality as f64) * scale_mult;
    let probe = layout(grid, aspect, 1.0);
    let current_long = probe.tile_w.max(probe.tile_h) as f64;
    layout(grid, aspect, (long_side / current_long).clamp(0.2, 4.0))
}

impl SheetLayout {
    pub fn tile_origin(&self, idx: u32) -> (u32, u32) {
        let col = idx % self.cols;
        let row = idx / self.cols;
        (
            self.pad_x + col * (self.tile_w + self.gap),
            self.grid_top + row * (self.tile_h + self.gap),
        )
    }
}

// ---------------------------------------------------------------- drawing

fn fill_rect(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, c: Rgb<u8>) {
    for yy in y..(y + h).min(img.height()) {
        for xx in x..(x + w).min(img.width()) {
            img.put_pixel(xx, yy, c);
        }
    }
}

fn stroke_rect(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, t: u32, c: Rgb<u8>) {
    fill_rect(img, x, y, w, t, c);
    fill_rect(img, x, y + h.saturating_sub(t), w, t, c);
    fill_rect(img, x, y, t, h, c);
    fill_rect(img, x + w.saturating_sub(t), y, t, h, c);
}

/// Push the corner pixels of a rect back to `bg` outside radius `r`
/// (the fixture's border-radius:3px, PRD FR10 / design fidelity NFR6).
fn round_corners(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, r: u32, bg: Rgb<u8>) {
    if r == 0 {
        return;
    }
    let rf = r as f32;
    let centers = [
        (x + r, y + r),
        (x + w - 1 - r, y + r),
        (x + r, y + h - 1 - r),
        (x + w - 1 - r, y + h - 1 - r),
    ];
    let corners = [
        (x, y),
        (x + w - r, y),
        (x, y + h - r),
        (x + w - r, y + h - r),
    ];
    for ((cx, cy), (ox, oy)) in centers.iter().zip(corners.iter()) {
        for yy in *oy..(*oy + r).min(img.height()) {
            for xx in *ox..(*ox + r).min(img.width()) {
                let dx = xx as f32 - *cx as f32;
                let dy = yy as f32 - *cy as f32;
                let d = (dx * dx + dy * dy).sqrt();
                if d > rf + 0.5 {
                    img.put_pixel(xx, yy, bg);
                } else if d > rf - 0.5 {
                    let a = d - (rf - 0.5);
                    blend(img.get_pixel_mut(xx, yy), bg, a);
                }
            }
        }
    }
}

pub struct HeaderMeta<'a> {
    pub title: &'a str,
    pub duration: String,
    pub resolution: String,
    pub fps: String,
}

/// Render the chrome (card bg, border, header band, tile borders) once; tile
/// pixels get blitted into the slots per frame.
pub fn render_chrome(l: &SheetLayout, fonts: &Fonts, hm: &HeaderMeta) -> RgbImage {
    let mut img = RgbImage::from_pixel(l.card_w, l.card_h, theme::CARD);
    let s = l.scale as f32;

    // Card border
    stroke_rect(
        &mut img,
        0,
        0,
        l.card_w,
        l.card_h,
        l.hairline,
        theme::BORDER,
    );

    // --- header band ---
    let x0 = l.pad_x as f32;
    let y0 = l.pad_y as f32;
    let inner_w = (l.card_w - 2 * l.pad_x) as f32;

    // Right-aligned meta columns: label (9px semibold dim uppercase) over
    // value (10.5px medium mint), gap 14 CSS between columns.
    let label_px = 9.0 * s;
    let value_px = 10.5 * s;
    let tracking = label_px * 0.06;
    let col_gap = 14.0 * s;
    let fields = [
        ("DURATION", hm.duration.as_str()),
        ("RESOLUTION", hm.resolution.as_str()),
        ("FPS", hm.fps.as_str()),
    ];
    let col_ws: Vec<f32> = fields
        .iter()
        .map(|(lab, val)| {
            measure_text(&fonts.semibold, label_px, tracking, lab).max(measure_text(
                &fonts.medium,
                value_px,
                0.0,
                val,
            ))
        })
        .collect();
    let meta_w: f32 = col_ws.iter().sum::<f32>() + col_gap * (fields.len() as f32 - 1.0);
    let label_h = label_px * 1.3;
    let value_gap = 3.0 * s;

    let mut cx = x0 + inner_w - meta_w;
    for ((lab, val), w) in fields.iter().zip(col_ws.iter()) {
        // Right-align both lines inside the column (numbers right-aligned per design)
        let lw = measure_text(&fonts.semibold, label_px, tracking, lab);
        let vw = measure_text(&fonts.medium, value_px, 0.0, val);
        draw_text(
            &mut img,
            &fonts.semibold,
            label_px,
            theme::TEXT_DIM,
            cx + w - lw,
            y0,
            tracking,
            lab,
        );
        draw_text(
            &mut img,
            &fonts.medium,
            value_px,
            theme::ACCENT,
            cx + w - vw,
            y0 + label_h + value_gap,
            0.0,
            val,
        );
        cx += w + col_gap;
    }

    // Title: bottom-aligned with the meta block, ellipsized (PRD FR11)
    let title_px = 15.0 * s;
    let title_h = title_px * 1.3;
    let block_h = CSS_HEADER_BLOCK_H as f32 * s;
    let title_max_w = inner_w - meta_w - 20.0 * s;
    let title = ellipsize(&fonts.bold, title_px, hm.title, title_max_w);
    draw_text(
        &mut img,
        &fonts.bold,
        title_px,
        theme::TEXT,
        x0,
        y0 + block_h - title_h,
        0.0,
        &title,
    );

    // Divider under the header
    let div_y = l.pad_y + ((CSS_HEADER_BLOCK_H + CSS_HEADER_PAD_B) * l.scale).round() as u32;
    fill_rect(
        &mut img,
        l.pad_x,
        div_y,
        l.card_w - 2 * l.pad_x,
        l.hairline,
        theme::BORDER_STRONG,
    );

    // Tile wells + borders
    for i in 0..(l.cols * l.rows) {
        let (tx, ty) = l.tile_origin(i);
        fill_rect(&mut img, tx, ty, l.tile_w, l.tile_h, theme::SURFACE2);
        stroke_rect(
            &mut img,
            tx,
            ty,
            l.tile_w,
            l.tile_h,
            l.hairline,
            theme::BORDER,
        );
        round_corners(
            &mut img,
            tx,
            ty,
            l.tile_w,
            l.tile_h,
            (CSS_TILE_RADIUS * l.scale).round() as u32,
            theme::CARD,
        );
    }

    img
}

/// Blit one tile image into its slot (inside the 1px border), re-rounding corners.
pub fn blit_tile(img: &mut RgbImage, l: &SheetLayout, idx: u32, tile: &RgbImage) {
    let (tx, ty) = l.tile_origin(idx);
    let b = l.hairline;
    let iw = l.tile_w - 2 * b;
    let ih = l.tile_h - 2 * b;
    let src = if tile.width() != iw || tile.height() != ih {
        image::imageops::resize(tile, iw, ih, image::imageops::FilterType::Triangle)
    } else {
        tile.clone()
    };
    for (sy, row) in src.rows().enumerate() {
        for (sx, p) in row.enumerate() {
            img.put_pixel(tx + b + sx as u32, ty + b + sy as u32, *p);
        }
    }
    stroke_rect(img, tx, ty, l.tile_w, l.tile_h, b, theme::BORDER);
    round_corners(
        img,
        tx,
        ty,
        l.tile_w,
        l.tile_h,
        (CSS_TILE_RADIUS * l.scale).round() as u32,
        theme::CARD,
    );
}

/// Timestamp pill, bottom-right of a tile (PRD FR9).
pub fn draw_timestamp(img: &mut RgbImage, l: &SheetLayout, idx: u32, fonts: &Fonts, text: &str) {
    let s = l.scale as f32;
    let px = 10.0 * s;
    let pad_x = 5.0 * s;
    let pad_y = 2.5 * s;
    let margin = 6.0 * s;
    let tw = measure_text(&fonts.medium, px, 0.0, text);
    let bw = (tw + 2.0 * pad_x).round() as u32;
    let bh = (px * 1.3 + 2.0 * pad_y).round() as u32;
    let (tx, ty) = l.tile_origin(idx);
    let bx = tx + l.tile_w - bw - margin.round() as u32;
    let by = ty + l.tile_h - bh - margin.round() as u32;
    // Semi-transparent dark pill
    for yy in by..(by + bh).min(img.height()) {
        for xx in bx..(bx + bw).min(img.width()) {
            blend(img.get_pixel_mut(xx, yy), Rgb([0, 0, 0]), 0.62);
        }
    }
    draw_text(
        img,
        &fonts.medium,
        px,
        theme::TEXT,
        bx as f32 + pad_x,
        by as f32 + pad_y,
        0.0,
        text,
    );
}

pub fn fmt_timestamp(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}
