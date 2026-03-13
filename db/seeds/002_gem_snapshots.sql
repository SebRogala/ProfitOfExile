-- Seed data for gem_snapshots hypertable
-- Covers: transfigured vs base gems, multiple variants, varying listing counts,
-- NULL chaos (missing data), boundary prices, low-listing edge case
--
-- Uses timestamps spread across 3 hours to test time_bucket aggregations.
-- Gem names match real PoE items for domain realism.

INSERT INTO gem_snapshots (time, name, variant, chaos, listings, is_transfigured, gem_color) VALUES

-- Kinetic Blast of Fragmentation (transfigured RED gem, two snapshots)
('2026-03-10 09:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', 177.00, 65, true, 'RED'),
('2026-03-10 10:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', 182.50, 58, true, 'RED'),
('2026-03-10 11:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', 170.00, 72, true, 'RED'),

-- Kinetic Blast base gem (not transfigured)
('2026-03-10 09:00:00+00', 'Kinetic Blast', '1/20', 7.00, 95, false, 'GREEN'),
('2026-03-10 10:00:00+00', 'Kinetic Blast', '1/20', 6.50, 102, false, 'GREEN'),

-- Ball Lightning of Orbiting (transfigured BLUE gem, 20/20 variant)
('2026-03-10 09:00:00+00', 'Ball Lightning of Orbiting', '20/20', 350.00, 12, true, 'BLUE'),
('2026-03-10 10:00:00+00', 'Ball Lightning of Orbiting', '20/20', 345.00, 14, true, 'BLUE'),

-- Empower Support (base, WHITE gem, multiple variants)
('2026-03-10 09:00:00+00', 'Empower Support', '1/0', 1.00, 407, false, 'WHITE'),
('2026-03-10 09:00:00+00', 'Empower Support', '3/0', 45.00, 120, false, 'WHITE'),
('2026-03-10 10:00:00+00', 'Empower Support', '3/0', 47.00, 115, false, 'WHITE'),

-- Vaal Grace (Vaal prefix gem, GREEN)
('2026-03-10 09:00:00+00', 'Vaal Grace', '20/20', 25.00, 88, false, 'GREEN'),

-- Edge case: low listings (confidence = LOW at < 5)
('2026-03-10 09:00:00+00', 'Arcanist Brand of Volatility', '20/20', 900.00, 3, true, 'BLUE'),

-- Edge case: NULL chaos (price unavailable)
('2026-03-10 09:00:00+00', 'Herald of Ice of Freezing', '1/20', NULL, 0, true, 'GREEN'),

-- Edge case: zero-value gem
('2026-03-10 09:00:00+00', 'Determination', '1/0', 0.00, 500, false, 'RED');
