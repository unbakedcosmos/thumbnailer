//! Design tokens from DESIGN.md — the one built-in template (PRD FR22).

use image::Rgb;

pub const BG: Rgb<u8> = Rgb([0x0b, 0x0c, 0x0f]);
pub const CARD: Rgb<u8> = Rgb([0x15, 0x16, 0x1a]);
pub const SURFACE2: Rgb<u8> = Rgb([0x1f, 0x21, 0x26]);
pub const BORDER: Rgb<u8> = Rgb([0x2a, 0x2c, 0x33]);
pub const BORDER_STRONG: Rgb<u8> = Rgb([0x2e, 0x30, 0x38]);
pub const TEXT: Rgb<u8> = Rgb([0xee, 0xf0, 0xf4]);
pub const TEXT_DIM: Rgb<u8> = Rgb([0x5b, 0x60, 0x6c]);
pub const ACCENT: Rgb<u8> = Rgb([0x9f, 0xe8, 0xb0]);

pub const FONT_REGULAR: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
pub const FONT_MEDIUM: &[u8] = include_bytes!("../fonts/JetBrainsMono-Medium.ttf");
pub const FONT_SEMIBOLD: &[u8] = include_bytes!("../fonts/JetBrainsMono-SemiBold.ttf");
pub const FONT_BOLD: &[u8] = include_bytes!("../fonts/JetBrainsMono-Bold.ttf");
