-- Drop the rows the widened constraint allowed before restoring it. transfigure_results
-- is derived analysis output — the next collection cycle recomputes it.
DELETE FROM transfigure_results WHERE confidence = 'NO_BASE';
ALTER TABLE transfigure_results DROP CONSTRAINT IF EXISTS transfigure_results_confidence_check;
ALTER TABLE transfigure_results ADD CONSTRAINT transfigure_results_confidence_check
    CHECK (confidence IN ('OK', 'LOW'));
