-- Seed data for currency_snapshots hypertable
-- Replaces former exchange_snapshots + gcp_snapshots seeds.
-- Covers: multiple currency types, multiple time points, volume variance,
-- sparkline change directions, NULL chaos edge case, zero-volume edge case.
--
-- currency_snapshots stores periodic price snapshots from poe.ninja
-- currency overview (chaos equivalent, trade volume, sparkline delta).

INSERT INTO currency_snapshots (time, currency_id, chaos, volume, sparkline_change) VALUES

-- Divine Orb (high value, high volume, price rising)
('2026-03-10 09:00:00+00', 'divine-orb',     168.00000000, 4523.0000,  2.35),
('2026-03-10 10:00:00+00', 'divine-orb',     169.50000000, 4410.0000,  2.50),
('2026-03-10 11:00:00+00', 'divine-orb',     170.00000000, 4385.0000,  2.80),

-- Exalted Orb (moderate value, moderate volume, price declining)
('2026-03-10 09:00:00+00', 'exalted-orb',     12.00000000, 1820.0000, -1.10),
('2026-03-10 10:00:00+00', 'exalted-orb',     11.80000000, 1795.0000, -1.45),
('2026-03-10 11:00:00+00', 'exalted-orb',     11.50000000, 1810.0000, -1.80),

-- Gemcutter's Prism (low value, carries over from old gcp_snapshots)
('2026-03-10 09:00:00+00', 'gemcutters-prism', 4.00000000,  950.0000,  0.00),
('2026-03-10 10:00:00+00', 'gemcutters-prism', 4.10000000,  940.0000,  0.25),
('2026-03-10 11:00:00+00', 'gemcutters-prism', 3.90000000,  965.0000, -0.15),

-- Vaal Orb (stable price, zero sparkline)
('2026-03-10 09:00:00+00', 'vaal-orb',         1.50000000,  620.0000,  0.00),
('2026-03-10 10:00:00+00', 'vaal-orb',         1.50000000,  615.0000,  0.00),

-- Edge case: NULL chaos (API returned no price)
('2026-03-10 11:00:00+00', 'vaal-orb',         NULL,           0.0000,  NULL),

-- Edge case: zero volume (no trades observed)
('2026-03-10 09:00:00+00', 'mirror-of-kalandra', 95000.00000000, 0.0000, 0.00);
