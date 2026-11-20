//! Offline OCR of 6-digit authenticator codes, using the pure-Rust `ocrs` engine.
//!
//! The model files are *not* compiled in: the JS side reads them from the extension's
//! own `models/` directory at runtime and hands the bytes to [`DigitReader::new`].
//!
//! The stock `ocrs` pipeline sometimes splits a thin `1` off a code into its own text
//! line, so instead of `OcrEngine::get_text` we merge all detected words that share a
//! row into a single, slightly padded line box before recognition.

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use rten_imageproc::{BoundingRect, RectF, RotatedRect};
use wasm_bindgen::prelude::{wasm_bindgen, JsError};

/// Digits per authenticator code.
const CODE_LEN: usize = 6;
/// Horizontal / vertical padding added around a merged row, relative to its height.
const PAD_X: f32 = 0.3;
const PAD_Y: f32 = 0.09;

/// Axis-aligned box as `[top, left, bottom, right]`.
type Box4 = [f32; 4];

/// Merge boxes whose vertical extents overlap by more than half of the smaller one.
fn merge_rows(mut boxes: Vec<Box4>) -> Vec<Box4> {
    boxes.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut rows: Vec<Box4> = Vec::new();
    for b in boxes {
        let same_row = rows.iter_mut().find(|r| {
            let overlap = r[2].min(b[2]) - r[0].max(b[0]);
            overlap > 0.5 * (r[2] - r[0]).min(b[2] - b[0])
        });
        match same_row {
            Some(r) => *r = [r[0].min(b[0]), r[1].min(b[1]), r[2].max(b[2]), r[3].max(b[3])],
            None => rows.push(b),
        }
    }
    rows
}

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

/// Reads 6-digit codes off RGBA frames.
#[wasm_bindgen]
pub struct DigitReader {
    engine: OcrEngine,
}

#[wasm_bindgen]
impl DigitReader {
    /// `detection` and `recognition` are the contents of the `.rten` model files.
    #[wasm_bindgen(constructor)]
    pub fn new(detection: Vec<u8>, recognition: Vec<u8>) -> Result<DigitReader, JsError> {
        let load = |bytes| Model::load(bytes).map_err(|e| JsError::new(&e.to_string()));
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(load(detection)?),
            recognition_model: Some(load(recognition)?),
            allowed_chars: Some("0123456789 ".to_string()),
            ..Default::default()
        })
        .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(DigitReader { engine })
    }

    /// Find a 6-digit code in an RGBA frame; `undefined` when there is none.
    pub fn read(&self, rgba: &[u8], width: u32, height: u32) -> Option<String> {
        let source = ImageSource::from_bytes(rgba, (width, height)).ok()?;
        let input = self.engine.prepare_input(source).ok()?;
        let words = self.engine.detect_words(&input).ok()?;
        let boxes = words
            .iter()
            .map(|w| {
                let b = w.bounding_rect();
                [b.top(), b.left(), b.bottom(), b.right()]
            })
            .collect();
        let rows: Vec<RotatedRect> = merge_rows(boxes)
            .into_iter()
            .map(|[top, left, bottom, right]| {
                let h = bottom - top;
                RotatedRect::from_rect(RectF::from_tlbr(
                    top - PAD_Y * h,
                    left - PAD_X * h,
                    bottom + PAD_Y * h,
                    right + PAD_X * h,
                ))
            })
            .collect();
        let lines = self.engine.find_text_lines(&input, &rows);
        self.engine
            .recognize_text(&input, &lines)
            .ok()?
            .into_iter()
            .flatten()
            .find_map(|line| extract_code(&line.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_code, merge_rows};

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

    #[test]
    fn rows_merge_words_on_the_same_line() {
        let rows = merge_rows(vec![
            [10.0, 100.0, 40.0, 140.0], // "456"
            [12.0, 10.0, 41.0, 20.0],   // thin "1"
            [12.0, 30.0, 40.0, 90.0],   // "23"
            [100.0, 10.0, 120.0, 90.0], // another line
        ]);
        assert_eq!(rows, vec![[10.0, 10.0, 41.0, 140.0], [100.0, 10.0, 120.0, 90.0]]);
    }
}
