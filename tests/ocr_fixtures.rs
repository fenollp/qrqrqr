//! End-to-end OCR regression tests on real webcam captures (see tests/fixtures/README.md).
//!
//! Needs the OCR model: run `make models` (the `make test` target does it for you).

use image::{ImageBuffer, Rgba};
use qrqrqr::DigitReader;
use std::{fs, path::Path};

const MODEL: &str = "extension/models/text-recognition.rten";

fn reader() -> DigitReader {
    let model = fs::read(MODEL).unwrap_or_else(|e| panic!("{MODEL}: {e} (run `make models`)"));
    DigitReader::new(model).ok().expect("model loads")
}

fn read_png(reader: &DigitReader, name: &str) -> Option<String> {
    let path = Path::new("tests/fixtures").join(name);
    let img = image::open(&path).unwrap_or_else(|e| panic!("{path:?}: {e}")).into_rgba8();
    let (w, h) = img.dimensions();
    reader.read(img.as_raw(), w, h)
}

#[test]
fn reads_the_code_from_real_phone_captures() {
    let reader = reader();
    for name in ["phone-frame.png", "phone-roi.png", "phone-blurred-frame.png", "phone-blurred-roi.png"] {
        assert_eq!(read_png(&reader, name).as_deref(), Some("729592"), "{name}");
    }
}

#[test]
fn reads_nothing_where_there_is_no_code() {
    let reader = reader();
    let blank = ImageBuffer::from_pixel(640, 480, Rgba([128u8, 128, 128, 255]));
    assert_eq!(reader.read(blank.as_raw(), 640, 480), None);
    // deterministic noise
    let mut seed = 1u32;
    let noise = ImageBuffer::from_fn(640, 480, |_, _| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let v = (seed >> 24) as u8;
        Rgba([v, v, v, 255])
    });
    assert_eq!(reader.read(noise.as_raw(), 640, 480), None);
    assert_eq!(reader.read(&[], 0, 0), None);
}

#[test]
fn colour_polarity_does_not_matter() {
    // The same phone frame with inverted colours (dark digits on a light screen).
    let reader = reader();
    let img = image::open("tests/fixtures/phone-frame.png").unwrap().into_rgba8();
    let (w, h) = img.dimensions();
    let inverted: Vec<u8> =
        img.as_raw().chunks_exact(4).flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], p[3]]).collect();
    assert_eq!(reader.read(&inverted, w, h).as_deref(), Some("729592"));
}
