//! Reads the Faustus exchange's "COMPETING TRADES" header band, its top
//! Competing Trades row and the I want / I have name boxes off one pressed
//! frame, and turns them into one Companion selection or one reader error.
use std::sync::{Arc, Mutex};

use super::parse::{ExchangeReadError, ExchangeSelection};

/// The OCR boundary: `crate::ocr::recognize_lines` in production. Each call is
/// handed one prepared crop and answers its lines in that image's pixels.
pub type RecognizeLines =
    Box<dyn FnMut(&image::DynamicImage) -> Result<Vec<crate::ocr::OcrLineBox>, String> + Send>;

/// What one exchange press read.
pub type ExchangeReadResult = Result<ExchangeSelection, ExchangeReadError>;

/// The per-press slot `consume` fills, held by the press's finish callback.
#[derive(Clone, Default)]
pub struct ExchangeReadHandle(Arc<Mutex<Option<ExchangeReadResult>>>);

impl ExchangeReadHandle {
    /// The press's result, once; `None` when nothing was consumed.
    pub fn take(&self) -> Option<ExchangeReadResult> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// One exchange press, prepared and waiting for its frame.
pub struct ExchangePending {
    standing: Option<crate::ssot::ScreenSlice>,
    ocr: RecognizeLines,
    result: ExchangeReadHandle,
    panel: Option<[i32; 4]>,
    scale: f32,
    probe_error: bool,
}

/// Prepares one press. `standing` is the published screen slice at press
/// time; the pressed frame's own geometry decides whether it applies.
pub fn prepare(
    standing: Option<crate::ssot::ScreenSlice>,
    ocr: RecognizeLines,
) -> (ExchangePending, ExchangeReadHandle) {
    let result = ExchangeReadHandle::default();
    (ExchangePending { standing, ocr, result: result.clone(), panel: None, scale: 1.0, probe_error: false }, result)
}

impl ExchangePending {
    fn read_crop(&mut self, frame: &crate::capture::Capture, rect: [i32; 4]) -> Result<Vec<crate::ocr::OcrLineBox>, String> {
        let [px, py, _, _] = self.panel.ok_or("exchange panel unplaced")?;
        let [dx, dy, w, h] = rect.map(|v| (v as f32 * self.scale).round() as i32);
        let (x, y) = (px + dx, py + dy);
        if x < 0 || y < 0 || w <= 0 || h <= 0
            || i64::from(x) + i64::from(w) > i64::from(frame.image.width())
            || i64::from(y) + i64::from(h) > i64::from(frame.image.height())
        {
            return Err("exchange crop outside pressed frame".into());
        }
        let crop = frame.image.crop_imm(x as u32, y as u32, w as u32, h as u32);
        let prepared = crate::capture::preprocess_for_ocr_rows(&crop);
        (self.ocr)(&prepared)
    }

    fn selection(&mut self, frame: &crate::capture::Capture) -> ExchangeReadResult {
        use super::parse::{parse_top_row, ExchangeReadError::*};
        if self.probe_error {
            return Err(RatioUnreadable);
        }
        let mut row = self.read_crop(frame, [290, 342, 197, 26]).map_err(|_| RatioUnreadable)?;
        row.sort_by_key(|word| word.x);
        let split = row.windows(2).enumerate()
            .max_by_key(|(_, pair)| i64::from(pair[1].x) - i64::from(pair[0].x) - i64::from(pair[0].w))
            .map(|(index, _)| index + 1).ok_or(RatioUnreadable)?;
        let (ratio, stock) = row.split_at(split);
        let ratio = ratio.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" ");
        let stock = stock.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" ");
        let mut want = self.read_crop(frame, [65, 111, 190, 28]).map_err(|_| NamesUnreadable)?;
        let mut have = self.read_crop(frame, [565, 103, 160, 46]).map_err(|_| NamesUnreadable)?;
        want.sort_by_key(|line| (line.y, line.x));
        have.sort_by_key(|line| (line.y, line.x));
        let want: Vec<_> = want.iter().map(|line| line.text.as_str()).collect();
        let have: Vec<_> = have.iter().map(|line| line.text.as_str()).collect();
        parse_top_row(&ratio, &stock, &want, &have)
    }
}

impl<C> crate::read_hotkey::PendingRead<C> for ExchangePending {
    fn probe(&mut self, frame: &Arc<crate::capture::Capture>, _ctx: &C) -> Result<bool, String> {
        let screen = self.standing.filter(|_| crate::ssot::screen_matches(
            &self.standing, (frame.image.width(), frame.image.height()), frame.origin, frame.client,
        )).unwrap_or_else(|| crate::ssot::screen_from_capture(frame));
        self.panel = crate::ssot::placements(&screen).exchange.map(|placement| placement.panel);
        self.scale = screen.ui_scale / crate::ssot::ui_scale_for_height(1080);
        match self.read_crop(frame, [295, 298, 185, 21]) {
            Ok(lines) => Ok(lines.iter().flat_map(|line| line.text.split_whitespace()).collect::<Vec<_>>().join(" ") == "COMPETING TRADES"),
            Err(_) => {
                self.probe_error = true;
                Ok(true)
            }
        }
    }

    fn consume(mut self: Box<Self>, frame: Result<&Arc<crate::capture::Capture>, &str>, _ctx: &C) {
        let selection = frame.map_err(|_| ExchangeReadError::RatioUnreadable).and_then(|frame| self.selection(frame));
        *self.result.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{preprocess_for_ocr_rows, Capture, CaptureError, GrabTiming};
    use crate::exchange_read::parse::{Currency, Mode, OneSide};
    use crate::ocr::OcrLineBox;
    use crate::read_hotkey::{dispatch_press, DispatchOutcome, PreparedPress, ReaderId};
    use image::{DynamicImage, ImageBuffer, Rgb};
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct Fixture {
        rows: HashMap<String, Row>,
    }

    #[derive(Deserialize, Clone)]
    struct Row {
        ratio: String,
        stock: String,
        want: Vec<String>,
        have: Vec<String>,
    }

    fn fixture_row(id: &str) -> Row {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/exchange/rows.json")).unwrap();
        fixture.rows.get(id).unwrap_or_else(|| panic!("rows.json has no row {id}")).clone()
    }

    /// The 1920x1080 fullscreen measurement's crops, x/y/w/h in client px. The
    /// next row sits one 23 px row pitch below the top one; it is placed here
    /// only so a read of it is recognised and answers a different, valid row.
    const CROPS: [(&str, [u32; 4]); 5] = [
        ("header", [535, 413, 185, 21]),
        ("top-row", [530, 457, 197, 26]),
        ("next-row", [530, 480, 197, 26]),
        ("want", [305, 226, 190, 28]),
        ("have", [805, 218, 160, 46]),
    ];

    /// A grey-noise fullscreen 1080p frame: every crop's pixels differ, so the
    /// OCR stand-in can tell which placed crop it was handed.
    fn frame() -> Capture {
        let mut seed = 0x2545_f491_u32;
        let image = ImageBuffer::from_fn(1920, 1080, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = (seed >> 24) as u8;
            Rgb([v, v, v])
        });
        Capture {
            image: DynamicImage::ImageRgb8(image),
            monitor_id: 7,
            origin: (0, 0),
            client: [0, 0, 1920, 1080],
            timing: GrabTiming::default(),
        }
    }

    /// One OCR line in a prepared crop's pixels: 3x the crop plus the 40 px pad.
    fn line(text: &str, crop_x: i32, crop_y: i32, crop_w: i32) -> OcrLineBox {
        OcrLineBox { text: text.into(), x: 40 + 3 * crop_x, y: 40 + 3 * crop_y, w: 3 * crop_w, h: 3 * 14 }
    }

    /// The top-row crop as OCR answers it: the stock column first and split
    /// into its words, the ratio at the row's left, so only their positions
    /// say which is which.
    fn row_lines(ratio: &str, stock: &str) -> Vec<OcrLineBox> {
        let mut lines: Vec<OcrLineBox> =
            stock.split_whitespace().enumerate().map(|(i, word)| line(word, 158 + 12 * i as i32, 6, 10)).collect();
        lines.push(line(ratio, 4, 6, 60));
        lines
    }

    fn name_lines(lines: &[String]) -> Vec<OcrLineBox> {
        lines.iter().enumerate().map(|(i, text)| line(text, 2, 1 + 16 * i as i32, 150)).collect()
    }

    /// The OCR answers for a press over `row`, the next row a valid different one.
    fn answers(header: Result<Vec<OcrLineBox>, String>, row: &Row) -> HashMap<&'static str, Result<Vec<OcrLineBox>, String>> {
        HashMap::from([
            ("header", header),
            ("top-row", Ok(row_lines(&row.ratio, &row.stock))),
            ("next-row", Ok(row_lines("160 : 1", "4"))),
            ("want", Ok(name_lines(&row.want))),
            ("have", Ok(name_lines(&row.have))),
        ])
    }

    fn header_found() -> Result<Vec<OcrLineBox>, String> {
        Ok(vec![line("COMPETING TRADES", 10, 2, 160)])
    }

    struct Press {
        outcome: DispatchOutcome,
        result: Option<ExchangeReadResult>,
        /// The placed crops OCR was handed, in order; anything else is "unplaced".
        ocr_calls: Vec<&'static str>,
    }

    /// One exchange press through the real dispatch: one grab of [`frame`],
    /// the reader's probe and consume, OCR answered per recognised crop.
    fn press(mut answers: HashMap<&'static str, Result<Vec<OcrLineBox>, String>>) -> Press {
        let frame = frame();
        let placed: Vec<(&'static str, DynamicImage)> = CROPS
            .iter()
            .map(|(name, [x, y, w, h])| (*name, preprocess_for_ocr_rows(&frame.image.crop_imm(*x, *y, *w, *h))))
            .collect();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let ocr: RecognizeLines = Box::new(move |img: &DynamicImage| {
            let name = placed.iter().find(|(_, crop)| crop == img).map_or("unplaced", |(name, _)| *name);
            seen.lock().unwrap().push(name);
            answers.remove(name).unwrap_or(Ok(Vec::new()))
        });
        let (pending, handle) = prepare(None, ocr);
        let prepared: PreparedPress<()> = PreparedPress {
            delay: std::time::Duration::ZERO,
            pendings: vec![(ReaderId::Exchange, Box::new(pending))],
            dismissed: Vec::new(),
            finish: None,
            log: None,
            sweep: None,
        };
        let mut grab = Some(frame);
        let outcome = dispatch_press(
            prepared,
            &(),
            &mut |_| {},
            &mut || grab.take().ok_or_else(|| CaptureError::Failed("a second grab".into())),
            &mut |_| panic!("an exchange press carries no dismissal"),
        );
        let ocr_calls = calls.lock().unwrap().clone();
        Press { outcome, result: handle.take(), ocr_calls }
    }

    mod keep_happy {
        use super::*;

        // keep:happy
        #[test]
        fn the_placed_crops_of_one_pressed_frame_become_the_fixture_selection() {
            let ember_for_chaos = |stock| ExchangeSelection {
                mode: Mode::Sell,
                currency: Currency::Chaos,
                ratio: 130.0,
                one_side: OneSide::Right,
                left: 130.0,
                right: 1.0,
                stock,
                want_name: "Chaos Orb".into(),
                have_name: "Allflame Ember of the Ethereal".into(),
            };
            let cases = [("ember-for-chaos", ember_for_chaos(1)), ("ember-for-chaos-split-stock", ember_for_chaos(8125))];
            let mismatches: Vec<String> = cases
                .into_iter()
                .filter_map(|(id, expected)| {
                    let read = press(answers(header_found(), &fixture_row(id)));
                    let want = (DispatchOutcome::Selected(ReaderId::Exchange), Some(Ok(expected)));
                    let got = (read.outcome, read.result);
                    (got != want).then(|| format!("{id}: expected {want:?}, got {got:?} after OCR of {:?}", read.ocr_calls))
                })
                .collect();
            assert!(mismatches.is_empty(), "{mismatches:#?}");
        }
    }

    mod keep_critical {
        use super::*;

        // keep:critical — silent: a frame without the exchange panel (closed, or
        // an Alt-expanded tooltip over the header) that still reads a row fills
        // the Companion from whatever pixels sit where the row would be.
        #[test]
        fn a_frame_without_the_competing_trades_header_selects_nothing_and_reads_no_row() {
            let row = fixture_row("ember-for-chaos");
            for header in [Ok(Vec::new()), Ok(vec![line("Allflame Ember of the Ethereal", 10, 2, 160)])] {
                let read = press(answers(header.clone(), &row));

                assert_eq!(
                    (read.outcome, read.result, read.ocr_calls),
                    (DispatchOutcome::NoMatch, None, vec!["header"]),
                    "header OCR {header:?}",
                );
            }
        }

        // keep:critical — silent: a refused `<`/`>`-prefixed top row that falls
        // through to row 2 fills a competing order that is not the one to match.
        #[test]
        fn a_prefixed_top_row_is_ratio_unreadable_without_reading_the_next_row() {
            let read = press(answers(header_found(), &fixture_row("outbid-prefixed-top-row")));

            assert_eq!(
                (read.outcome, read.result),
                (DispatchOutcome::Selected(ReaderId::Exchange), Some(Err(ExchangeReadError::RatioUnreadable))),
            );
            assert!(!read.ocr_calls.contains(&"next-row"), "OCR read the next row: {:?}", read.ocr_calls);
        }

        // keep:critical — silent: a probe OCR error escaping as a failed dispatch
        // is reported as a grab failure, which tells the player the capture broke
        // when the panel was there and its text was unreadable.
        #[test]
        fn a_header_ocr_failure_is_an_unreadable_ratio_not_a_failed_dispatch() {
            let read = press(answers(Err("OCR recognize failed: engine busy".into()), &fixture_row("ember-for-chaos")));

            assert_eq!(
                (read.outcome, read.result),
                (DispatchOutcome::Selected(ReaderId::Exchange), Some(Err(ExchangeReadError::RatioUnreadable))),
            );
        }
    }
}
