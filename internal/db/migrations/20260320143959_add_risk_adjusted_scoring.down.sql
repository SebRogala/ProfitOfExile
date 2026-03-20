ALTER TABLE gem_signals DROP COLUMN IF EXISTS sell_confidence;
ALTER TABLE gem_signals DROP COLUMN IF EXISTS quick_sell_price;
ALTER TABLE gem_signals DROP COLUMN IF EXISTS risk_adjusted_value;

ALTER TABLE gem_features DROP COLUMN IF EXISTS stability_discount;
ALTER TABLE gem_features DROP COLUMN IF EXISTS sell_probability_factor;
