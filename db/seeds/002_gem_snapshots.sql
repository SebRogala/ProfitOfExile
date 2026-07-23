-- Seed data for gem_snapshots hypertable
-- Covers: transfigured vs base gems, multiple variants, varying listing counts,
-- NULL chaos (missing data), boundary prices, low-listing edge case,
-- corrupted vs uncorrupted (is_corrupted flag in PK)
--
-- Uses timestamps spread across 3 hours to test time_bucket aggregations.
-- Gem names match real PoE items for domain realism.

INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, chaos, listings, is_transfigured, gem_color) VALUES

-- Kinetic Blast of Fragmentation (transfigured RED gem, three snapshots, uncorrupted)
('Mirage', '2026-03-10 09:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', false, 177.00, 65, true, 'RED'),
('Mirage', '2026-03-10 10:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', false, 182.50, 58, true, 'RED'),
('Mirage', '2026-03-10 11:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', false, 170.00, 72, true, 'RED'),

-- Kinetic Blast of Fragmentation (corrupted — same gem/variant, different is_corrupted PK)
('Mirage', '2026-03-10 09:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', true, 55.00, 18, true, 'RED'),
('Mirage', '2026-03-10 10:00:00+00', 'Kinetic Blast of Fragmentation', '1/20', true, 52.00, 21, true, 'RED'),

-- Kinetic Blast base gem (not transfigured, BLUE gem per gem_colors seed)
('Mirage', '2026-03-10 09:00:00+00', 'Kinetic Blast', '1/20', false, 7.00, 95, false, 'BLUE'),
('Mirage', '2026-03-10 10:00:00+00', 'Kinetic Blast', '1/20', false, 6.50, 102, false, 'BLUE'),

-- Ball Lightning of Orbiting (transfigured BLUE gem, 20/20 variant)
('Mirage', '2026-03-10 09:00:00+00', 'Ball Lightning of Orbiting', '20/20', false, 350.00, 12, true, 'BLUE'),
('Mirage', '2026-03-10 10:00:00+00', 'Ball Lightning of Orbiting', '20/20', false, 345.00, 14, true, 'BLUE'),

-- Empower Support (base, RED gem per gem_colors seed, multiple variants)
('Mirage', '2026-03-10 09:00:00+00', 'Empower Support', '1/0', false, 1.00, 407, false, 'RED'),
('Mirage', '2026-03-10 09:00:00+00', 'Empower Support', '3/0', false, 45.00, 120, false, 'RED'),
('Mirage', '2026-03-10 10:00:00+00', 'Empower Support', '3/0', false, 47.00, 115, false, 'RED'),

-- Empower Support corrupted (corrupted gems often trade at different prices)
('Mirage', '2026-03-10 09:00:00+00', 'Empower Support', '3/0', true, 38.00, 85, false, 'RED'),

-- Vaal Grace (Vaal prefix gem, GREEN — corrupted by nature since Vaal gems are always corrupted)
('Mirage', '2026-03-10 09:00:00+00', 'Vaal Grace', '20/20', true, 25.00, 88, false, 'GREEN'),

-- Edge case: low listings (confidence = LOW at < 5), corrupted
('Mirage', '2026-03-10 09:00:00+00', 'Arcanist Brand of Volatility', '20/20', true, 750.00, 2, true, 'BLUE'),

-- Edge case: low listings uncorrupted for comparison
('Mirage', '2026-03-10 09:00:00+00', 'Arcanist Brand of Volatility', '20/20', false, 900.00, 3, true, 'BLUE'),

-- Edge case: NULL chaos (price unavailable)
('Mirage', '2026-03-10 09:00:00+00', 'Herald of Ice of Freezing', '1/20', false, NULL, 0, true, 'GREEN'),

-- Edge case: zero-value gem
('Mirage', '2026-03-10 09:00:00+00', 'Determination', '1/0', false, 0.00, 500, false, 'RED');
