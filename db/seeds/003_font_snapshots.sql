-- Seed data for font_snapshots hypertable
-- Covers: all 3 primary colors (RED/GREEN/BLUE), multiple variants,
-- multiple time points for aggregation testing.
--
-- Values modeled on real Font of Divine Skill analysis output.

INSERT INTO font_snapshots (league, time, color, variant, pool, ev) VALUES

-- RED color, 1/20 variant (3 snapshots for time_bucket testing)
('Mirage', '2026-03-10 09:00:00+00', 'RED', '1/20', 33, 26.00),
('Mirage', '2026-03-10 10:00:00+00', 'RED', '1/20', 33, 28.50),
('Mirage', '2026-03-10 11:00:00+00', 'RED', '1/20', 33, 24.00),

-- GREEN color, 1/20 variant
('Mirage', '2026-03-10 09:00:00+00', 'GREEN', '1/20', 28, 18.00),
('Mirage', '2026-03-10 10:00:00+00', 'GREEN', '1/20', 28, 19.50),

-- BLUE color, 1/20 variant
('Mirage', '2026-03-10 09:00:00+00', 'BLUE', '1/20', 25, 22.00),
('Mirage', '2026-03-10 10:00:00+00', 'BLUE', '1/20', 25, 21.00),

-- RED color, 20/20 variant (higher quality = better EV)
('Mirage', '2026-03-10 09:00:00+00', 'RED', '20/20', 33, 45.00),
('Mirage', '2026-03-10 10:00:00+00', 'RED', '20/20', 33, 48.00),

-- Edge case: single-gem pool
('Mirage', '2026-03-10 09:00:00+00', 'BLUE', '20/0', 1, 15.00);
