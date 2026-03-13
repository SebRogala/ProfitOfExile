-- Seed data for font_snapshots hypertable
-- Covers: all 3 primary colors (RED/GREEN/BLUE), multiple variants,
-- multiple time points for aggregation testing, edge case min=max
--
-- Values modeled on real Font of Divine Skill analysis output.

INSERT INTO font_snapshots (time, color, variant, pool, ev, min_val, max_val) VALUES

-- RED color, 1/20 variant (3 snapshots for time_bucket testing)
('2026-03-10 09:00:00+00', 'RED', '1/20', 33, 26.00, 3.00, 120.00),
('2026-03-10 10:00:00+00', 'RED', '1/20', 33, 28.50, 3.00, 135.00),
('2026-03-10 11:00:00+00', 'RED', '1/20', 33, 24.00, 3.00, 110.00),

-- GREEN color, 1/20 variant
('2026-03-10 09:00:00+00', 'GREEN', '1/20', 28, 18.00, 1.00, 85.00),
('2026-03-10 10:00:00+00', 'GREEN', '1/20', 28, 19.50, 1.00, 90.00),

-- BLUE color, 1/20 variant
('2026-03-10 09:00:00+00', 'BLUE', '1/20', 25, 22.00, 2.00, 95.00),
('2026-03-10 10:00:00+00', 'BLUE', '1/20', 25, 21.00, 2.00, 92.00),

-- RED color, 20/20 variant (higher quality = better EV)
('2026-03-10 09:00:00+00', 'RED', '20/20', 33, 45.00, 5.00, 200.00),
('2026-03-10 10:00:00+00', 'RED', '20/20', 33, 48.00, 5.00, 210.00),

-- Edge case: min equals max (single gem in pool worth something)
('2026-03-10 09:00:00+00', 'BLUE', '20/0', 1, 15.00, 15.00, 15.00);
