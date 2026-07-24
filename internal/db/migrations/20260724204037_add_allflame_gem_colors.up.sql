-- Allflame league adds four Strength/Intelligence transfigured skills. All are
-- classified RED (gem_colors carries a single color per gem; per operator call).
-- Idempotent: name is the PK, so re-runs are a no-op.
INSERT INTO gem_colors (name, color) VALUES
    ('Divine Blast of Radiance', 'RED'),
    ('Holy Hammers of Spirals', 'RED'),
    ('Holy Sweep of Hammerfalls', 'RED'),
    ('Reap of Butchery', 'RED')
ON CONFLICT (name) DO NOTHING;
