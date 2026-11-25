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
pub struct ExchangeReadHandle(Arc<Mutex<ReadSlot>>);

#[derive(Default)]
struct ReadSlot {
    read: Option<ExchangeReadResult>,
    diagnostic: RatioDiagnostic,
}

pub(crate) struct RatioDiagnostic {
    pub(crate) branch: &'static str,
    pub(crate) row: Option<String>,
}

impl Default for RatioDiagnostic {
    fn default() -> Self {
        Self { branch: "no-result", row: None }
    }
}

impl ExchangeReadHandle {
    /// The press's result, once; `None` when nothing was consumed.
    pub fn take(&self) -> Option<ExchangeReadResult> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).read.take()
    }

    pub(crate) fn take_detailed(&self) -> (Option<ExchangeReadResult>, RatioDiagnostic) {
        let slot = std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()));
        (slot.read, slot.diagnostic)
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
    diagnostic: RatioDiagnostic,
}

/// Prepares one press. `standing` is the published screen slice at press
/// time; the pressed frame's own geometry decides whether it applies.
pub fn prepare(
    standing: Option<crate::ssot::ScreenSlice>,
    ocr: RecognizeLines,
) -> (ExchangePending, ExchangeReadHandle) {
    let result = ExchangeReadHandle::default();
    (ExchangePending { standing, ocr, result: result.clone(), panel: None, scale: 1.0, probe_error: false, diagnostic: RatioDiagnostic::default() }, result)
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
        use super::parse::{parse_top_row_detailed, ExchangeReadError::*};
        if self.probe_error {
            self.diagnostic.branch = "probe_error";
            return Err(RatioUnreadable);
        }
        self.diagnostic.branch = "row-crop-ocr-error";
        let mut row = self.read_crop(frame, [290, 342, 197, 26]).map_err(|_| RatioUnreadable)?;
        row.sort_by_key(|line| (line.x, line.y));
        let row = row.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join(" ");
        self.diagnostic.row = Some(row.clone());
        let (ratio, stock) = ratio_and_stock(&row);
        let mut want = self.read_crop(frame, [65, 111, 190, 28]).map_err(|_| NamesUnreadable)?;
        let mut have = self.read_crop(frame, [565, 103, 160, 46]).map_err(|_| NamesUnreadable)?;
        want.sort_by_key(|line| (line.y, line.x));
        have.sort_by_key(|line| (line.y, line.x));
        let want: Vec<_> = want.iter().map(|line| line.text.as_str()).collect();
        let have: Vec<_> = have.iter().map(|line| line.text.as_str()).collect();
        let mut branch = None;
        let result = parse_top_row_detailed(ratio, stock, &want, &have, &mut branch);
        if let Some(branch) = branch { self.diagnostic.branch = branch; }
        result
    }
}

/// Splits the top row's text, its OCR lines joined left to right, after the
/// ratio's `a : b`: OCR answers lines, not words, and may return the ratio and
/// the stock as one line or as two. The remainder is the stock.
fn ratio_and_stock(row: &str) -> (&str, &str) {
    let Some(colon) = row.find(':') else { return (row, "") };
    let after = &row[colon + 1..];
    let start = after.len() - after.trim_start().len();
    let end = after[start..].find(char::is_whitespace).map_or(after.len(), |i| start + i);
    row.split_at(colon + 1 + end)
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
        let selection = match frame {
            Ok(frame) => self.selection(frame),
            Err(_) => {
                self.diagnostic = RatioDiagnostic::default();
                Err(ExchangeReadError::RatioUnreadable)
            }
        };
        *self.result.0.lock().unwrap_or_else(|e| e.into_inner()) = ReadSlot {
            read: Some(selection), diagnostic: self.diagnostic,
        };
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

    /// How OCR grouped the top row: the measured ratio and stock as two lines,
    /// or both as one line.
    #[derive(Debug, Clone, Copy)]
    enum RowShape {
        TwoLines,
        OneLine,
    }

    /// The top-row crop as `recognize_lines` answers it, one box per line. Two
    /// lines come back stock first, so only their positions say which is which.
    fn row_lines(ratio: &str, stock: &str, shape: RowShape) -> Vec<OcrLineBox> {
        match shape {
            RowShape::TwoLines => vec![line(stock, 158, 6, 34), line(ratio, 4, 6, 60)],
            RowShape::OneLine => vec![line(&format!("{ratio} {stock}"), 4, 6, 188)],
        }
    }

    fn name_lines(lines: &[String]) -> Vec<OcrLineBox> {
        lines.iter().enumerate().map(|(i, text)| line(text, 2, 1 + 16 * i as i32, 150)).collect()
    }

    /// The OCR answers for a press over `row`, the next row a valid different one.
    fn answers(
        header: Result<Vec<OcrLineBox>, String>,
        row: &Row,
        shape: RowShape,
    ) -> HashMap<&'static str, Result<Vec<OcrLineBox>, String>> {
        HashMap::from([
            ("header", header),
            ("top-row", Ok(row_lines(&row.ratio, &row.stock, shape))),
            ("next-row", Ok(row_lines("160 : 1", "4", shape))),
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
    fn press(answers: HashMap<&'static str, Result<Vec<OcrLineBox>, String>>) -> Press {
        let (outcome, handle, ocr_calls) = dispatched(answers);
        Press { outcome, result: handle.take(), ocr_calls }
    }

    /// [`press`] with its result slot left unread, for the finish path.
    fn dispatched(
        mut answers: HashMap<&'static str, Result<Vec<OcrLineBox>, String>>,
    ) -> (DispatchOutcome, ExchangeReadHandle, Vec<&'static str>) {
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
        (outcome, handle, ocr_calls)
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
                .flat_map(|case| [RowShape::TwoLines, RowShape::OneLine].map(|shape| (case.clone(), shape)))
                .filter_map(|((id, expected), shape)| {
                    let read = press(answers(header_found(), &fixture_row(id), shape));
                    let want = (DispatchOutcome::Selected(ReaderId::Exchange), Some(Ok(expected)));
                    let got = (read.outcome, read.result);
                    (got != want).then(|| format!("{id} {shape:?}: expected {want:?}, got {got:?} after OCR of {:?}", read.ocr_calls))
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
                let read = press(answers(header.clone(), &row, RowShape::TwoLines));

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
            for shape in [RowShape::TwoLines, RowShape::OneLine] {
                let read = press(answers(header_found(), &fixture_row("outbid-prefixed-top-row"), shape));

                assert_eq!(
                    (read.outcome, read.result),
                    (DispatchOutcome::Selected(ReaderId::Exchange), Some(Err(ExchangeReadError::RatioUnreadable))),
                    "{shape:?}",
                );
                assert!(!read.ocr_calls.contains(&"next-row"), "{shape:?}: OCR read the next row: {:?}", read.ocr_calls);
            }
        }

        // keep:critical — silent: a probe OCR error escaping as a failed dispatch
        // is reported as a grab failure, which tells the player the capture broke
        // when the panel was there and its text was unreadable.
        #[test]
        fn a_header_ocr_failure_is_an_unreadable_ratio_not_a_failed_dispatch() {
            let read = press(answers(
                Err("OCR recognize failed: engine busy".into()),
                &fixture_row("ember-for-chaos"),
                RowShape::TwoLines,
            ));

            assert_eq!(
                (read.outcome, read.result),
                (DispatchOutcome::Selected(ReaderId::Exchange), Some(Err(ExchangeReadError::RatioUnreadable))),
            );
        }

        // keep:critical — silent: a failed read leaves no evidence in app.log,
        // so "Ratio unreadable" cannot be traced to the row OCR read or the
        // refusal that fired.
        #[test]
        fn a_failed_read_logs_its_row_text_and_branch() {
            use crate::exchange_read::{finish_logged, RATIO_SIDE_NOT_POSITIVE_DECIMAL};
            let row = Row { ratio: "220 : l".into(), stock: "3".into(), ..fixture_row("ember-for-chaos") };
            let (outcome, handle, _) = dispatched(answers(header_found(), &row, RowShape::TwoLines));

            let (mut events, mut logs) = (Vec::new(), Vec::new());
            finish_logged(
                outcome,
                &handle,
                1_700_000_000_123,
                &mut |event| events.push(serde_json::to_value(event).unwrap()),
                &mut |line| logs.push(line),
            );

            assert_eq!(events, vec![serde_json::json!({"pressedAtMs": 1_700_000_000_123_u64, "error": "ratio-unreadable"})]);
            assert!(
                matches!(logs.as_slice(), [line] if line.contains(RATIO_SIDE_NOT_POSITIVE_DECIMAL) && line.contains("220 : l")),
                "expected exactly one diagnostic line naming {RATIO_SIDE_NOT_POSITIVE_DECIMAL} and the row text \"220 : l\", got {logs:?}",
            );
        }
    }
}
