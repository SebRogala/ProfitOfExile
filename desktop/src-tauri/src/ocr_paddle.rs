//! POE-507 spike only (never lands): PaddleOCR PP-OCRv6 as the off-Windows
//! engine behind `crate::ocr`, run through ONNX Runtime (`ort`, load-dynamic).
//!
//! Enabled only when `POE_OCR_ENGINE=paddle`; otherwise the off-Windows stub
//! keeps returning `UNAVAILABLE`, so the default test run is unchanged.
//! `POE_PADDLE_DET` / `POE_PADDLE_REC` name the official `*_onnx` model dirs
//! (`inference.onnx`; the rec dir also holds `dict.txt`, the `character_dict`
//! of its `inference.yml`, one entry per line), and `ORT_DYLIB_PATH` the
//! onnxruntime shared library.
//!
//! Detection and recognition follow PaddleX's `OCR.yaml` pipeline defaults:
//! DB resize to multiples of 32 with min side 64 and max side 4000, ImageNet
//! normalisation, thresh 0.3, box_thresh 0.6, unclip 1.5; recognition at
//! height 48, width = max(320, 48·w/h) capped at 3200, (x/255−0.5)/0.5, CTC
//! with blank 0 and a trailing space. Boxes are axis-aligned (PoE UI text is
//! horizontal), so the polygon unclip becomes a rect grow by area·r/perimeter.

use std::sync::{Mutex, OnceLock};

use image::{imageops::FilterType, DynamicImage, RgbImage};
use ort::{environment::Environment, session::Session, value::Tensor};

use crate::ocr::OcrLineBox;

pub struct Engine {
    _env: Environment,
    det: Session,
    rec: Session,
    dict: Vec<String>,
}

static ENGINE: OnceLock<Result<Mutex<Engine>, String>> = OnceLock::new();

pub fn enabled() -> bool {
    std::env::var("POE_OCR_ENGINE").map(|v| v == "paddle").unwrap_or(false)
}

fn load() -> Result<Engine, String> {
    let var = |k: &str| std::env::var(k).map_err(|_| format!("{k} not set"));
    let env = ort::init_from(var("ORT_DYLIB_PATH")?)
        .map_err(|e| format!("onnxruntime: {e}"))?
        .with_name("poe-paddle")
        .build()
        .map_err(|e| format!("ort env: {e}"))?;
    let threads = std::env::var("POE_PADDLE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    let open = |dir: String| -> Result<Session, String> {
        Session::builder(&env)
            .map_err(|e| e.to_string())?
            .with_intra_threads(threads)
            .map_err(|e| e.to_string())?
            .commit_from_file(format!("{dir}/inference.onnx"))
            .map_err(|e| format!("{dir}: {e}"))
    };
    let det = open(var("POE_PADDLE_DET")?)?;
    let rec_dir = var("POE_PADDLE_REC")?;
    let rec = open(rec_dir.clone())?;
    let dict_txt = std::fs::read_to_string(format!("{rec_dir}/dict.txt")).map_err(|e| format!("dict: {e}"))?;
    let mut dict = vec![String::new()]; // CTC blank
    dict.extend(dict_txt.lines().map(str::to_string));
    dict.push(" ".to_string());
    Ok(Engine { _env: env, det, rec, dict })
}

fn engine() -> Result<&'static Mutex<Engine>, String> {
    ENGINE.get_or_init(|| load().map(Mutex::new)).as_ref().map_err(Clone::clone)
}

pub fn ready() -> Result<(), String> {
    engine().map(|_| ())
}

pub fn recognize_lines(img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
    let mut eng = engine()?.lock().unwrap_or_else(|e| e.into_inner());
    let rgb = img.to_rgb8();
    let boxes = eng.detect(&rgb)?;
    let mut out = Vec::new();
    for [x, y, w, h] in boxes {
        let crop = image::imageops::crop_imm(&rgb, x as u32, y as u32, w as u32, h as u32).to_image();
        let text = eng.recognize(&crop)?;
        if !text.trim().is_empty() {
            out.push(OcrLineBox { text, x, y, w, h });
        }
    }
    Ok(out)
}

pub fn recognize_text(img: &DynamicImage) -> Result<Vec<String>, String> {
    Ok(recognize_lines(img)?.into_iter().map(|l| l.text).collect())
}

impl Engine {
    fn detect(&mut self, rgb: &RgbImage) -> Result<Vec<[i32; 4]>, String> {
        let (w0, h0) = rgb.dimensions();
        if w0 < 2 || h0 < 2 {
            return Ok(Vec::new());
        }
        let mut ratio = 1.0f32;
        let min_side = w0.min(h0) as f32;
        if min_side < 64.0 {
            ratio = 64.0 / min_side;
        }
        let max_side = w0.max(h0) as f32 * ratio;
        if max_side > 4000.0 {
            ratio *= 4000.0 / max_side;
        }
        let rw = (((w0 as f32 * ratio) / 32.0).round() as u32).max(1) * 32;
        let rh = (((h0 as f32 * ratio) / 32.0).round() as u32).max(1) * 32;
        let resized = image::imageops::resize(rgb, rw, rh, FilterType::Triangle);
        // PaddleX decodes BGR and normalises channel-wise in that order.
        let mean = [0.406f32, 0.456, 0.485];
        let std = [0.225f32, 0.224, 0.229];
        let plane = (rw * rh) as usize;
        let mut data = vec![0f32; 3 * plane];
        for (i, p) in resized.pixels().enumerate() {
            let bgr = [p[2], p[1], p[0]];
            for c in 0..3 {
                data[c * plane + i] = (bgr[c] as f32 / 255.0 - mean[c]) / std[c];
            }
        }
        let input = Tensor::from_array(([1usize, 3, rh as usize, rw as usize], data)).map_err(|e| e.to_string())?;
        let outputs = self.det.run(ort::inputs![input]).map_err(|e| e.to_string())?;
        let (_, prob) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (pw, ph) = (rw as usize, rh as usize);
        let bitmap: Vec<bool> = prob.iter().map(|&v| v > 0.3).collect();
        let mut seen = vec![false; plane];
        let mut boxes = Vec::new();
        let mut stack = Vec::new();
        for start in 0..plane {
            if !bitmap[start] || seen[start] {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
            seen[start] = true;
            stack.push(start);
            while let Some(i) = stack.pop() {
                let (x, y) = (i % pw, i / pw);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= pw as i32 || ny >= ph as i32 {
                            continue;
                        }
                        let j = ny as usize * pw + nx as usize;
                        if bitmap[j] && !seen[j] {
                            seen[j] = true;
                            stack.push(j);
                        }
                    }
                }
            }
            let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
            if bw.min(bh) < 3.0 {
                continue;
            }
            let mut sum = 0f32;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    sum += prob[y * pw + x];
                }
            }
            if sum / (bw * bh) < 0.6 {
                continue;
            }
            let d = bw * bh * 1.5 / (2.0 * (bw + bh));
            let (ex0, ey0, ex1, ey1) = (x0 as f32 - d, y0 as f32 - d, x1 as f32 + 1.0 + d, y1 as f32 + 1.0 + d);
            if (ex1 - ex0).min(ey1 - ey0) < 5.0 {
                continue;
            }
            let sx = w0 as f32 / rw as f32;
            let sy = h0 as f32 / rh as f32;
            let bx0 = (ex0 * sx).round().clamp(0.0, w0 as f32) as i32;
            let by0 = (ey0 * sy).round().clamp(0.0, h0 as f32) as i32;
            let bx1 = (ex1 * sx).round().clamp(0.0, w0 as f32) as i32;
            let by1 = (ey1 * sy).round().clamp(0.0, h0 as f32) as i32;
            if bx1 - bx0 >= 2 && by1 - by0 >= 2 {
                boxes.push([bx0, by0, bx1 - bx0, by1 - by0]);
            }
        }
        // PaddleX order: top-to-bottom, then left-to-right within 10 px rows.
        boxes.sort_by_key(|b| (b[1], b[0]));
        for i in 1..boxes.len() {
            let mut j = i;
            while j > 0 && (boxes[j][1] - boxes[j - 1][1]).abs() < 10 && boxes[j][0] < boxes[j - 1][0] {
                boxes.swap(j, j - 1);
                j -= 1;
            }
        }
        Ok(boxes)
    }

    fn recognize(&mut self, crop: &RgbImage) -> Result<String, String> {
        let (w, h) = crop.dimensions();
        let ratio = w as f32 / h as f32;
        let img_w = ((48.0 * ratio.max(320.0 / 48.0)) as usize).min(3200);
        let rw = ((48.0 * ratio).ceil() as usize).clamp(1, img_w);
        let resized = image::imageops::resize(crop, rw as u32, 48, FilterType::Triangle);
        let plane = 48 * img_w;
        let mut data = vec![0f32; 3 * plane];
        for (x, y, p) in resized.enumerate_pixels() {
            let bgr = [p[2], p[1], p[0]];
            for c in 0..3 {
                data[c * plane + y as usize * img_w + x as usize] = (bgr[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
        let input = Tensor::from_array(([1usize, 3, 48, img_w], data)).map_err(|e| e.to_string())?;
        let outputs = self.rec.run(ort::inputs![input]).map_err(|e| e.to_string())?;
        let (shape, logits) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (steps, classes) = (shape[1] as usize, shape[2] as usize);
        let mut text = String::new();
        let mut prev = 0usize;
        for t in 0..steps {
            let row = &logits[t * classes..(t + 1) * classes];
            let best = row.iter().enumerate().fold((0, f32::MIN), |a, (i, &v)| if v > a.1 { (i, v) } else { a }).0;
            if best != 0 && best != prev {
                if let Some(ch) = self.dict.get(best) {
                    text.push_str(ch);
                }
            }
            prev = best;
        }
        Ok(text)
    }
}

/// Spike test harness: skips when the engine is off and prints one
/// `PADDLE-SPIKE|test|PASS/FAIL|...` row per test for the report.
#[cfg(test)]
pub mod spike {
    use std::cell::RefCell;
    use std::time::Instant;

    pub fn off(test: &str) -> bool {
        if super::enabled() {
            return false;
        }
        eprintln!("PADDLE-SPIKE|{test}|SKIP|engine off");
        true
    }

    #[derive(Default)]
    pub struct Clock(pub RefCell<Vec<f64>>);

    impl Clock {
        pub fn time<T>(&self, f: impl FnOnce() -> T) -> T {
            let t = Instant::now();
            let out = f();
            self.0.borrow_mut().push(t.elapsed().as_secs_f64() * 1000.0);
            out
        }
    }

    pub fn report(test: &str, pass: bool, ocr: &Clock, total_ms: f64, detail: &str) {
        let mut v = ocr.0.borrow().clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pick = |q: f64| if v.is_empty() { 0.0 } else { v[((v.len() - 1) as f64 * q).round() as usize] };
        eprintln!(
            "PADDLE-SPIKE|{test}|{}|ocr_calls={}|ocr_ms_median={:.0}|ocr_ms_p95={:.0}|read_ms={:.0}|{detail}",
            if pass { "PASS" } else { "FAIL" },
            v.len(),
            pick(0.5),
            pick(0.95),
            total_ms
        );
    }
}
