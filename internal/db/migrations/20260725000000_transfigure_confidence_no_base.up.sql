-- Allow the NO_BASE confidence written by AnalyzeTransfigure for transfigured
-- gems whose base gem is absent from the snapshot (ROI unknown, not zero).
--
-- NOT VALID: every existing row was written as 'OK' or 'LOW' by an older binary,
-- so there is nothing to validate. Skipping the scan keeps this off the
-- ACCESS EXCLUSIVE path over a hypertable that retains full league history and
-- compresses chunks after 7 days. New writes are still checked.
ALTER TABLE transfigure_results DROP CONSTRAINT IF EXISTS transfigure_results_confidence_check;
ALTER TABLE transfigure_results ADD CONSTRAINT transfigure_results_confidence_check
    CHECK (confidence IN ('OK', 'LOW', 'NO_BASE')) NOT VALID;
