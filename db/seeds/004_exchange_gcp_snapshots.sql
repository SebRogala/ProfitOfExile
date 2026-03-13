-- Seed data for exchange_snapshots and gcp_snapshots hypertables
-- Covers: empower/enlighten/enhance entries, multiple time points,
-- varying listings, GCP price tracking
--
-- exchange_snapshots tracks support gem prices from poe.ninja exchange view.
-- gcp_snapshots tracks Gemcutter's Prism chaos equivalent.

INSERT INTO exchange_snapshots (time, name, chaos, listings) VALUES

-- Empower Support (high demand, stable price)
('2026-03-10 09:00:00+00', 'empower', 45.00, 407),
('2026-03-10 10:00:00+00', 'empower', 46.00, 395),
('2026-03-10 11:00:00+00', 'empower', 44.50, 412),

-- Enlighten Support (moderate demand)
('2026-03-10 09:00:00+00', 'enlighten', 38.00, 285),
('2026-03-10 10:00:00+00', 'enlighten', 39.00, 278),

-- Enhance Support (lower demand, fewer listings)
('2026-03-10 09:00:00+00', 'enhance', 12.00, 150),
('2026-03-10 10:00:00+00', 'enhance', 11.50, 145),

-- Edge case: NULL chaos (API returned no price)
('2026-03-10 11:00:00+00', 'enhance', NULL, 0);


INSERT INTO gcp_snapshots (time, chaos) VALUES

-- GCP price over 3 snapshots (tracks gradual price movement)
('2026-03-10 09:00:00+00', 4.00),
('2026-03-10 10:00:00+00', 4.10),
('2026-03-10 11:00:00+00', 3.90);
