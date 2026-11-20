//! Offline QR decoding for the qrqrqr Firefox extension.
//!
//! The JS side grabs a camera frame into a canvas and passes its raw RGBA
//! pixels here. Everything runs locally; no network access is involved.

mod ocr;

pub use ocr::DigitReader;

use wasm_bindgen::prelude::wasm_bindgen;

/// Convert RGBA pixels to 8-bit luma (BT.601 integer approximation).
fn to_luma(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let n = width * height;
    let mut out = Vec::with_capacity(n);
    for px in rgba.chunks_exact(4).take(n) {
        let (r, g, b) = (px[0] as u32, px[1] as u32, px[2] as u32);
        out.push(((77 * r + 150 * g + 29 * b) >> 8) as u8);
    }
    out
}

fn decode_luma(luma: &[u8], width: usize, height: usize) -> Option<String> {
    let mut img =
        rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| luma[y * width + x]);
    for grid in img.detect_grids() {
        if let Ok((_meta, content)) = grid.decode() {
            return Some(content);
        }
    }
    None
}

/// Decode the first readable QR code in an RGBA frame.
///
/// Returns `undefined` when no code is found. Light-on-dark (inverted)
/// codes are tried as a fallback.
#[wasm_bindgen]
pub fn decode_rgba(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let mut luma = to_luma(rgba, w, h);
    if let Some(s) = decode_luma(&luma, w, h) {
        return Some(s);
    }
    luma.iter_mut().for_each(|p| *p = 255 - *p);
    decode_luma(&luma, w, h)
}

#[cfg(test)]
mod tests {
    use super::decode_rgba;
    use qrcode::{Color, QrCode};

    fn render(text: &str, invert: bool) -> (Vec<u8>, u32, u32) {
        let code = QrCode::new(text.as_bytes()).unwrap();
        let modules = code.width();
        let scale = 6;
        let quiet = 4;
        let size = (modules + 2 * quiet) * scale;
        let colors = code.to_colors();
        let mut rgba = vec![0u8; size * size * 4];
        for y in 0..size {
            for x in 0..size {
                let (mx, my) = (x / scale, y / scale);
                let dark = mx >= quiet
                    && my >= quiet
                    && mx < quiet + modules
                    && my < quiet + modules
                    && colors[(my - quiet) * modules + (mx - quiet)] == Color::Dark;
                let v = if dark ^ invert { 0 } else { 255 };
                let i = (y * size + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        (rgba, size as u32, size as u32)
    }

    #[test]
    fn decodes_plain_code() {
        let (px, w, h) = render("https://example.org/héllo?x=1", false);
        assert_eq!(
            decode_rgba(&px, w, h).as_deref(),
            Some("https://example.org/héllo?x=1")
        );
    }

    #[test]
    fn decodes_inverted_code() {
        let (px, w, h) = render("WIFI:T:WPA;S:home;P:secret;;", true);
        assert_eq!(
            decode_rgba(&px, w, h).as_deref(),
            Some("WIFI:T:WPA;S:home;P:secret;;")
        );
    }

    #[test]
    fn empty_frame_is_none() {
        assert_eq!(decode_rgba(&vec![255; 64 * 64 * 4], 64, 64), None);
        assert_eq!(decode_rgba(&[], 0, 0), None);
    }
}
