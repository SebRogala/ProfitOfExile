-- Restore the narrower constraint. NOT VALID again, and deliberately without a
-- DELETE of the NO_BASE rows: transfigure_results is derived output that the next
-- collection cycle recomputes, and a DELETE keyed on a non-segmentby column would
-- decompress every chunk to evaluate it.
--
-- Note this is not a strict inverse everywhere: production carries no confidence
-- CHECK at all (its table predates the one in the create migration), so rolling
-- back there adds a constraint rather than restoring one.
ALTER TABLE transfigure_results DROP CONSTRAINT IF EXISTS transfigure_results_confidence_check;
ALTER TABLE transfigure_results ADD CONSTRAINT transfigure_results_confidence_check
    CHECK (confidence IN ('OK', 'LOW')) NOT VALID;
