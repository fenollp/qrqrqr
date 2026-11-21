//! Offline OCR of 6-digit authenticator codes, using the pure-Rust `ocrs` engine.
//!
//! Only `ocrs`' *recognition* model is used. Text localisation is done by [`crate::finder`]
//! (cheap image processing), because the stock text detector is both slow and unreliable on
//! webcam frames of a phone. The model file is not compiled in: the JS side reads it from the
//! extension's own `models/` directory at runtime and hands the bytes to [`DigitReader::new`].

use crate::finder::{find_digit_rows, Row};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use rten_imageproc::{PointF, RotatedRect, Vec2};
use wasm_bindgen::prelude::{wasm_bindgen, JsError};

/// Digits per authenticator code.
const CODE_LEN: usize = 6;
/// Padding around the glyphs of a row, relative to the row's height, as (along, across).
/// A tight crop reads clean text best (a loose one pulls in neighbouring lines); noisy or
/// low-contrast text sometimes needs more context, so a looser crop is the second attempt.
const PADS: [(f32, f32); 2] = [(0.35, 0.15), (0.5, 0.3)];

/// Accept recognized text only if it is exactly [`CODE_LEN`] digits, optionally split
/// in two halves by a space ("123 456"). Returns the digits without the space.
fn extract_code(text: &str) -> Option<String> {
    let t = text.trim();
    let digits: String = t.chars().filter(|c| *c != ' ').collect();
    let well_formed = digits.len() == CODE_LEN
        && digits.chars().all(|c| c.is_ascii_digit())
        && t.matches(' ').count() <= 1;
    well_formed.then_some(digits)
}

/// The recognizer's input for a row: an oriented, padded box.
fn row_rect(row: &Row, (pad_along, pad_across): (f32, f32)) -> RotatedRect {
    // `up` is perpendicular to the text direction, pointing to the top of the glyphs.
    let up = Vec2::from_xy(row.angle.sin(), -row.angle.cos());
    RotatedRect::new(
        PointF::from_yx(row.cy, row.cx),
        up,
        row.length + 2.0 * pad_along * row.height,
        row.height * (1.0 + 2.0 * pad_across),
    )
}

/// Reads 6-digit codes off RGBA frames.
#[wasm_bindgen]
pub struct DigitReader {
    engine: OcrEngine,
}

#[wasm_bindgen]
impl DigitReader {
    /// `recognition` is the content of the `text-recognition.rten` model file.
    #[wasm_bindgen(constructor)]
    pub fn new(recognition: Vec<u8>) -> Result<DigitReader, JsError> {
        let model = Model::load(recognition).map_err(|e| JsError::new(&e.to_string()))?;
        let engine = OcrEngine::new(OcrEngineParams {
            recognition_model: Some(model),
            ..Default::default()
        })
        .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(DigitReader { engine })
    }

    /// Find a 6-digit code in an RGBA frame; `undefined` when there is none.
    ///
    /// Candidate rows are read as they are first; if none of them is a code, once more with
    /// the colours inverted (the model was trained mostly on dark-on-light text). Each pass
    /// tries a tight crop of the row, then a looser one.
    pub fn read(&self, rgba: &[u8], width: u32, height: u32) -> Option<String> {
        let (w, h) = (width as usize, height as usize);
        if w == 0 || h == 0 || rgba.len() < w * h * 4 {
            return None;
        }
        let rows = find_digit_rows(&crate::to_luma(rgba, w, h), w, h);
        if rows.is_empty() {
            return None;
        }
        let dims = (width, height);
        let normal = self.recognize(rgba, dims, &rows);
        normal.or_else(|| {
            let inverted: Vec<u8> = rgba
                .chunks_exact(4)
                .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], p[3]])
                .collect();
            self.recognize(&inverted, dims, &rows)
        })
    }
}

impl DigitReader {
    fn recognize(&self, rgba: &[u8], dims: (u32, u32), rows: &[Row]) -> Option<String> {
        let input = self.engine.prepare_input(ImageSource::from_bytes(rgba, dims).ok()?).ok()?;
        PADS.iter().find_map(|&pads| {
            let lines: Vec<Vec<RotatedRect>> = rows.iter().map(|r| vec![row_rect(r, pads)]).collect();
            self.engine
                .recognize_text(&input, &lines)
                .ok()?
                .into_iter()
                .flatten()
                .find_map(|line| extract_code(&line.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::extract_code;

    #[test]
    fn code_accepts_plain_and_spaced() {
        assert_eq!(extract_code("123456").as_deref(), Some("123456"));
        assert_eq!(extract_code(" 123 456 ").as_deref(), Some("123456"));
    }

    #[test]
    fn code_rejects_wrong_shapes() {
        for bad in ["", "1", "23 456", "1234567", "12 34 56", "12a456", "1 2 3 4 5 6"] {
            assert_eq!(extract_code(bad), None, "{bad:?}");
        }
    }
}
