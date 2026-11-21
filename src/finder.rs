//! Locating rows of digits in a frame, without any neural network.
//!
//! The scene-text detector that ships with `ocrs` is built for documents and street scenes;
//! on a webcam frame showing a phone it returns dozens of spurious boxes and none around the
//! code, and it costs ~85% of the OCR time. A 2FA code is a short, evenly spaced row of
//! high-contrast glyphs of similar height, which classic image processing finds reliably:
//!
//! 1. threshold each pixel against its local mean (both polarities: light-on-dark and
//!    dark-on-light),
//! 2. take connected components of digit-like size and shape,
//! 3. chain neighbouring components of similar height into rows (a tilted phone is fine,
//!    because components are linked pairwise, not against a global baseline),
//! 4. fit a line through each row to get its centre, angle, length and height.
//!
//! The recognizer then only sees these rows.

/// Half-size of the window used for the local mean (in pixels of the analysed frame).
const WINDOW_RADIUS: usize = 16;
/// How much brighter/darker than its neighbourhood a pixel must be to count as ink.
const CONTRAST: i32 = 24;
/// Glyph height limits: absolute minimum, and maximum as a fraction of the frame height.
const MIN_GLYPH_H: f32 = 12.0;
const MAX_GLYPH_H_FRAC: f32 = 0.5;
/// A single digit is at most this wide relative to its height; a merged blob is counted as
/// `width / (DIGIT_PITCH_RATIO * height)` digits and may be up to MAX_BLOB_RATIO wide.
const SINGLE_DIGIT_MAX_RATIO: f32 = 1.1;
const DIGIT_PITCH_RATIO: f32 = 0.65;
const MAX_BLOB_RATIO: f32 = 3.2;
/// A row has this many glyphs (digits may touch and merge, or be missed).
const MIN_GLYPHS: usize = 4;
const MAX_GLYPHS: usize = 10;
/// Linking limits between two neighbouring glyphs, relative to the height of the first.
const MAX_GAP: f32 = 1.5;
const MAX_OVERLAP: f32 = 0.35;
const MAX_DY: f32 = 0.5;
const MAX_H_RATIO: f32 = 1.7;
/// Return at most this many rows (best first).
const MAX_ROWS: usize = 4;
/// Upper bound on connected components examined per polarity (clutter guard).
const MAX_COMPONENTS: usize = 3000;

/// A row of glyphs: an oriented box around them.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Centre of the box, in pixels.
    pub cx: f32,
    pub cy: f32,
    /// Direction of the text, radians, 0 = left-to-right, positive = clockwise (y points down).
    pub angle: f32,
    /// Extent along the text direction.
    pub length: f32,
    /// Extent across it.
    pub height: f32,
    /// Number of digits the row is estimated to contain.
    pub glyphs: usize,
}

#[derive(Clone, Copy, Debug)]
struct Glyph {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    /// How many digits this blob stands for: blurred or tilted neighbours touch and merge.
    count: usize,
    /// Label of this blob in the label map.
    id: u32,
    /// Ink pixel moments: n, Σx, Σy, Σx², Σxy, Σy².
    moments: [f64; 6],
}

impl Glyph {
    fn w(&self) -> f32 {
        self.right - self.left
    }
    fn h(&self) -> f32 {
        self.bottom - self.top
    }
    fn cx(&self) -> f32 {
        (self.left + self.right) / 2.0
    }
    fn cy(&self) -> f32 {
        (self.top + self.bottom) / 2.0
    }

    /// A lone digit is at most about as wide as tall; wider means touching digits.
    fn digits(&self) -> usize {
        let ratio = self.w() / self.h();
        if ratio <= SINGLE_DIGIT_MAX_RATIO {
            1
        } else {
            (ratio / DIGIT_PITCH_RATIO).round().max(2.0) as usize
        }
    }
}

/// Find rows of digit-like glyphs in an 8-bit grayscale image, best candidates first.
pub fn find_digit_rows(luma: &[u8], width: usize, height: usize) -> Vec<Row> {
    if width < 2 * WINDOW_RADIUS || height < 2 * WINDOW_RADIUS || luma.len() < width * height {
        return Vec::new();
    }
    let means = local_means(luma, width, height);
    let mut rows = Vec::new();
    for light_on_dark in [true, false] {
        let mut mask = ink_mask(luma, &means, light_on_dark);
        let (glyphs, labels) = glyphs(&mut mask, width, height);
        rows.extend(chain_rows(glyphs, &labels, width));
    }
    rows.sort_by(|a, b| score(b).total_cmp(&score(a)));
    rows.truncate(MAX_ROWS);
    rows
}

/// Prefer rows with six glyphs, then taller ones.
fn score(r: &Row) -> f32 {
    -1000.0 * (r.glyphs as f32 - 6.0).abs() + r.height
}

/// Mean of the (2R+1)² window around every pixel, via an integral image.
fn local_means(luma: &[u8], w: usize, h: usize) -> Vec<u8> {
    let stride = w + 1;
    let mut integral = vec![0u32; stride * (h + 1)];
    for y in 0..h {
        let mut run = 0u32;
        for x in 0..w {
            run += u32::from(luma[y * w + x]);
            integral[(y + 1) * stride + x + 1] = integral[y * stride + x + 1] + run;
        }
    }
    let mut means = vec![0u8; w * h];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(WINDOW_RADIUS), (y + WINDOW_RADIUS + 1).min(h));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(WINDOW_RADIUS), (x + WINDOW_RADIUS + 1).min(w));
            let sum = integral[y1 * stride + x1] + integral[y0 * stride + x0]
                - integral[y0 * stride + x1]
                - integral[y1 * stride + x0];
            means[y * w + x] = (sum / ((y1 - y0) * (x1 - x0)) as u32) as u8;
        }
    }
    means
}

fn ink_mask(luma: &[u8], means: &[u8], light_on_dark: bool) -> Vec<bool> {
    luma.iter()
        .zip(means)
        .map(|(&v, &m)| {
            let d = i32::from(v) - i32::from(m);
            if light_on_dark {
                d > CONTRAST
            } else {
                -d > CONTRAST
            }
        })
        .collect()
}

/// Connected components (8-neighbourhood) that look like a digit, and a label map (0 = no
/// ink, otherwise the blob's `id`). Consumes the mask.
fn glyphs(mask: &mut [bool], w: usize, h: usize) -> (Vec<Glyph>, Vec<u32>) {
    let mut labels = vec![0u32; w * h];
    let max_h = h as f32 * MAX_GLYPH_H_FRAC;
    let mut out = Vec::new();
    let mut stack = Vec::new();
    let mut components = 0;
    for start in 0..w * h {
        if !mask[start] {
            continue;
        }
        components += 1;
        if components > MAX_COMPONENTS {
            return (Vec::new(), labels); // far too busy to be a clean row of digits
        }
        let id = components as u32;
        mask[start] = false;
        labels[start] = id;
        stack.push(start);
        let (mut left, mut right, mut top, mut bottom) = (w, 0, h, 0);
        let mut area = 0usize;
        let mut m = [0f64; 6];
        while let Some(p) = stack.pop() {
            let (x, y) = (p % w, p / w);
            area += 1;
            let (fx, fy) = (x as f64, y as f64);
            m[0] += 1.0;
            m[1] += fx;
            m[2] += fy;
            m[3] += fx * fx;
            m[4] += fx * fy;
            m[5] += fy * fy;
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let n = ny * w + nx;
                    if mask[n] {
                        mask[n] = false;
                        labels[n] = id;
                        stack.push(n);
                    }
                }
            }
        }
        let g = Glyph {
            left: left as f32,
            top: top as f32,
            right: right as f32 + 1.0,
            bottom: bottom as f32 + 1.0,
            count: 1,
            id,
            moments: m,
        };
        let fill = area as f32 / (g.w() * g.h());
        if g.h() >= MIN_GLYPH_H
            && g.h() <= max_h
            && g.w() >= 0.08 * g.h()
            && g.w() <= MAX_BLOB_RATIO * g.h()
            && fill >= 0.12
        {
            out.push(Glyph { count: g.digits(), ..g });
        }
    }
    (out, labels)
}

/// Link glyphs pairwise (best pairs first, one successor and one predecessor each) and
/// turn every chain of plausible length into a [`Row`].
fn chain_rows(mut glyphs: Vec<Glyph>, labels: &[u32], width: usize) -> Vec<Row> {
    glyphs.sort_by(|a, b| a.cx().total_cmp(&b.cx()));
    let mut pairs = Vec::new();
    for (i, a) in glyphs.iter().enumerate() {
        for (j, b) in glyphs.iter().enumerate().skip(i + 1) {
            let gap = b.left - a.right;
            if gap > MAX_GAP * a.h() {
                if b.left - a.left > 4.0 * a.h() {
                    break; // sorted by cx: everything further right is even farther
                }
                continue;
            }
            let ratio = a.h().max(b.h()) / a.h().min(b.h());
            let dy = (b.cy() - a.cy()).abs();
            if gap >= -MAX_OVERLAP * a.h() && dy <= MAX_DY * a.h() && ratio <= MAX_H_RATIO {
                pairs.push((gap.abs() / a.h() + dy / a.h(), i, j));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut next = vec![None; glyphs.len()];
    let mut has_prev = vec![false; glyphs.len()];
    for (_, i, j) in pairs {
        if next[i].is_none() && !has_prev[j] {
            next[i] = Some(j);
            has_prev[j] = true;
        }
    }
    let mut rows = Vec::new();
    for start in (0..glyphs.len()).filter(|&i| !has_prev[i]) {
        let mut chain = vec![glyphs[start]];
        let mut at = start;
        while let Some(n) = next[at] {
            chain.push(glyphs[n]);
            at = n;
        }
        if (MIN_GLYPHS..=MAX_GLYPHS).contains(&chain.iter().map(|g| g.count).sum()) {
            rows.push(fit_row(&chain, labels, width));
        }
    }
    rows
}

/// Orientation of a row from the principal axis of all its ink pixels (a row of digits is
/// long and thin, so this is far more accurate than a line through a few blob centres),
/// then the oriented bounding box of its blobs.
fn fit_row(chain: &[Glyph], labels: &[u32], width: usize) -> Row {
    let digits = chain.iter().map(|g| g.count).sum();
    let mut m = [0f64; 6];
    for g in chain {
        for (acc, v) in m.iter_mut().zip(g.moments) {
            *acc += v;
        }
    }
    let n = m[0].max(1.0);
    let (mx, my) = (m[1] / n, m[2] / n);
    let (vxx, vxy, vyy) = (m[3] / n - mx * mx, m[4] / n - mx * my, m[5] / n - my * my);
    let angle = (0.5 * (2.0 * vxy).atan2(vxx - vyy)) as f32;
    let (dx, dy) = (angle.cos(), angle.sin());
    // Exact extents of the row's own ink in the rotated frame: along-text axis
    // d = (dx, dy), across axis n = (-dy, dx). (Projecting the blobs' bounding boxes instead
    // overshoots on tilted text and would pull neighbouring lines into the crop.)
    let ids: Vec<u32> = chain.iter().map(|g| g.id).collect();
    let x0 = chain.iter().map(|g| g.left as usize).min().unwrap_or(0);
    let x1 = chain.iter().map(|g| g.right as usize).max().unwrap_or(0);
    let y0 = chain.iter().map(|g| g.top as usize).min().unwrap_or(0);
    let y1 = chain.iter().map(|g| g.bottom as usize).max().unwrap_or(0);
    let (mut a0, mut a1, mut n0, mut n1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for y in y0..y1 {
        for x in x0..x1 {
            if ids.contains(&labels[y * width + x]) {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let (a, c) = (fx * dx + fy * dy, -fx * dy + fy * dx);
                a0 = a0.min(a - 0.5);
                a1 = a1.max(a + 0.5);
                n0 = n0.min(c - 0.5);
                n1 = n1.max(c + 0.5);
            }
        }
    }
    let (am, nm) = ((a0 + a1) / 2.0, (n0 + n1) / 2.0);
    Row {
        cx: am * dx - nm * dy,
        cy: am * dy + nm * dx,
        angle,
        length: a1 - a0,
        height: n1 - n0,
        glyphs: digits,
    }
}

#[cfg(test)]
mod tests {
    use super::{find_digit_rows, Row};

    /// A `w`×`h` frame of `bg` with `count` glyph-like boxes (`gw`×`gh`) every `pitch` px,
    /// starting at (x0, y0) and drifting down by `slope` px per px of x.
    fn frame(bg: u8, fg: u8, count: usize, slope: f32) -> (Vec<u8>, usize, usize) {
        let (w, h) = (320, 200);
        let mut img = vec![bg; w * h];
        for i in 0..count {
            let (gx, gy) = (60 + i * 30 + if i >= 3 { 14 } else { 0 }, 80.0 + slope * (i * 30) as f32);
            for y in gy as usize..gy as usize + 34 {
                for x in gx..gx + 18 {
                    // leave a hole so it is not a solid slab, like a digit's counter
                    let hole = (x - gx) > 5 && (x - gx) < 12 && (y - gy as usize) > 9 && (y - gy as usize) < 24;
                    if !hole {
                        img[y * w + x] = fg;
                    }
                }
            }
        }
        (img, w, h)
    }

    /// The best row (the other polarity may add spurious halo rows after it).
    fn only(rows: Vec<Row>) -> Row {
        assert!(!rows.is_empty());
        rows.into_iter().next().unwrap()
    }

    #[test]
    fn finds_light_on_dark_row() {
        let (img, w, h) = frame(20, 235, 6, 0.0);
        let r = only(find_digit_rows(&img, w, h));
        assert_eq!(r.glyphs, 6);
        assert!(r.angle.abs() < 0.02, "{r:?}");
        assert!((r.height - 34.0).abs() < 2.0 && r.length > 170.0 && r.length < 190.0, "{r:?}");
        assert!((r.cy - 97.0).abs() < 3.0, "{r:?}");
    }

    #[test]
    fn finds_dark_on_light_row() {
        let (img, w, h) = frame(235, 20, 6, 0.0);
        assert_eq!(only(find_digit_rows(&img, w, h)).glyphs, 6);
    }

    #[test]
    fn follows_a_tilted_row() {
        let (img, w, h) = frame(20, 235, 6, 0.2); // ~11 degrees
        let r = only(find_digit_rows(&img, w, h));
        assert_eq!(r.glyphs, 6);
        assert!((r.angle - 0.2f32.atan()).abs() < 0.05, "{r:?}");
    }

    #[test]
    fn ignores_short_rows_and_blank_frames() {
        let (img, w, h) = frame(20, 235, 3, 0.0);
        assert!(find_digit_rows(&img, w, h).is_empty());
        assert!(find_digit_rows(&vec![128; 320 * 200], 320, 200).is_empty());
        assert!(find_digit_rows(&[], 0, 0).is_empty());
    }
}

