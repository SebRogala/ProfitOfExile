-- 3.30 additions that reach poe.ninja's skill-gem feed with no gem_colors
-- row, so the colour resolver leaves them uncoloured. The Dedication pools
-- require a resolved colour, so an uncoloured gem silently leaves the pool,
-- its tier classification and the rankings.
--
-- Arcane Link is BLUE by the owner's ruling on POE-375; the basis is
-- inferred, not verified against game data.
-- Fireball of Radiation is a Fireball transfigure and matches Fireball's
-- seeded BLUE (the resolver would also inherit it from the base gem).
--
-- Idempotent: name is the PK, so re-runs are a no-op.
INSERT INTO gem_colors (name, color) VALUES
    ('Arcane Link', 'BLUE'),
    ('Fireball of Radiation', 'BLUE')
ON CONFLICT (name) DO NOTHING;
