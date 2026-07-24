-- Allow the NO_BASE confidence written by AnalyzeTransfigure for transfigured
-- gems whose base gem is absent from the snapshot (ROI unknown, not zero).
ALTER TABLE transfigure_results DROP CONSTRAINT IF EXISTS transfigure_results_confidence_check;
ALTER TABLE transfigure_results ADD CONSTRAINT transfigure_results_confidence_check
    CHECK (confidence IN ('OK', 'LOW', 'NO_BASE'));
