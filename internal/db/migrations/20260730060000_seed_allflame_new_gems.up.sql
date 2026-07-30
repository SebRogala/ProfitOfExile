-- 3.29 (Allflame) additions that reach poe.ninja's skill-gem feed with no
-- gem_colors row, so the colour resolver leaves them uncoloured. That matters
-- beyond a log line: the Dedication pools require a resolved colour, so an
-- uncoloured gem silently leaves the pool, its tier classification and the
-- rankings.
--
-- The four Pacts are Exceptional skill gems with no attribute requirement
-- (the 3.29 notes list an attribute for every other new skill but not for
-- these). WHITE records that as a fact rather than leaving it unresolved —
-- the craft transforms a gem into another "of the same colour", so a colourless
-- gem is correctly out of the pool, and it stops being reported as a gap.
--
-- Dark Bargain is 3.29's rename of Dark Pact and inherits its BLUE.
-- Mana-Infused Staff is announced as an "Intelligence/Strength Skill"; the
-- first-listed attribute is the gem's colour, which is how the four
-- "Strength/Intelligence" transfigured skills seeded on 2026-07-24 were
-- classified RED.
--
-- Idempotent: name is the PK, so re-runs are a no-op.
INSERT INTO gem_colors (name, color) VALUES
    ('Pact of Beidat', 'WHITE'),
    ('Pact of Ghorr', 'WHITE'),
    ('Pact of K''Tash', 'WHITE'),
    ('Pact of Lycia', 'WHITE'),
    ('Dark Bargain', 'BLUE'),
    ('Mana-Infused Staff', 'BLUE')
ON CONFLICT (name) DO NOTHING;
