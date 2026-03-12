-- Seed data for strategies table
-- WARNING: edit_tokens below are for development/testing only.
-- Production tokens must be cryptographically random.
--
-- Covers: leaf node, series wrapper, wrapper tree with conversion rules,
-- empty tree edge case, cross-league data

INSERT INTO strategies (id, edit_token, name, league, tree, created_at, updated_at) VALUES

-- 1. Simple leaf strategy: farming Divine Orbs in maps
(
    'a1b2c3d4-0001-4000-8000-000000000001',
    'tok_divine_maps_a1b2c3d4e5f6',
    'Divine Orb Mapping',
    'Mirage',
    '{
        "type": "leaf",
        "name": "T16 Map Farm",
        "series": 10,
        "inputs": [{"item": "Chaos Orb", "quantity": 3}],
        "outputs": [{"item": "Divine Orb", "quantity": 1, "probability": 0.05}],
        "duration_seconds": 300
    }',
    '2026-03-01 12:00:00+00',
    '2026-03-01 12:00:00+00'
),

-- 2. Wrapper strategy: Lab farming with enchant sub-strategies
(
    'a1b2c3d4-0002-4000-8000-000000000002',
    'tok_lab_farm_b3c4d5e6f7g8',
    'Merciless Lab Enchant Farm',
    'Mirage',
    '{
        "type": "wrapper",
        "name": "Lab Run",
        "series": 5,
        "children": [
            {
                "type": "leaf",
                "name": "Helmet Enchant",
                "series": 1,
                "inputs": [{"item": "Offering to the Goddess", "quantity": 1}],
                "outputs": [{"item": "Enchanted Helmet", "quantity": 1, "probability": 0.25}],
                "duration_seconds": 480
            },
            {
                "type": "leaf",
                "name": "Boot Enchant",
                "series": 1,
                "inputs": [],
                "outputs": [{"item": "Enchanted Boots", "quantity": 1, "probability": 0.33}],
                "duration_seconds": 0
            }
        ]
    }',
    '2026-03-05 08:30:00+00',
    '2026-03-10 14:15:00+00'
),

-- 3. Fragment set conversion strategy (includes conversion_rules for inventory cascade scenarios)
-- All four guardian fragments are produced so the conversion rule can fire
(
    'a1b2c3d4-0003-4000-8000-000000000003',
    'tok_shaper_frags_c5d6e7f8',
    'Shaper Fragment Farming',
    'Mirage',
    '{
        "type": "wrapper",
        "name": "Shaper Set Assembly",
        "series": 4,
        "children": [
            {
                "type": "leaf",
                "name": "Fragment of the Phoenix",
                "series": 1,
                "inputs": [{"item": "Chaos Orb", "quantity": 15}],
                "outputs": [{"item": "Fragment of the Phoenix", "quantity": 1, "probability": 1.0}],
                "duration_seconds": 0
            },
            {
                "type": "leaf",
                "name": "Fragment of the Hydra",
                "series": 1,
                "inputs": [{"item": "Chaos Orb", "quantity": 12}],
                "outputs": [{"item": "Fragment of the Hydra", "quantity": 1, "probability": 1.0}],
                "duration_seconds": 0
            },
            {
                "type": "leaf",
                "name": "Fragment of the Minotaur",
                "series": 1,
                "inputs": [{"item": "Chaos Orb", "quantity": 14}],
                "outputs": [{"item": "Fragment of the Minotaur", "quantity": 1, "probability": 1.0}],
                "duration_seconds": 0
            },
            {
                "type": "leaf",
                "name": "Fragment of the Chimera",
                "series": 1,
                "inputs": [{"item": "Chaos Orb", "quantity": 13}],
                "outputs": [{"item": "Fragment of the Chimera", "quantity": 1, "probability": 1.0}],
                "duration_seconds": 0
            }
        ],
        "conversion_rules": [
            {
                "inputs": [
                    {"item": "Fragment of the Phoenix", "quantity": 1},
                    {"item": "Fragment of the Hydra", "quantity": 1},
                    {"item": "Fragment of the Minotaur", "quantity": 1},
                    {"item": "Fragment of the Chimera", "quantity": 1}
                ],
                "output": {"item": "Shaper Set", "quantity": 1}
            }
        ]
    }',
    '2026-03-08 16:00:00+00',
    '2026-03-11 09:45:00+00'
),

-- 4. Edge case: empty tree (newly created strategy)
(
    'a1b2c3d4-0004-4000-8000-000000000004',
    'tok_empty_strat_d7e8f9g0',
    'Untitled Strategy',
    'Mirage',
    '{}',
    '2026-03-12 00:00:00+00',
    '2026-03-12 00:00:00+00'
),

-- 5. Font of Divine Skill strategy (probability-based transfigured gem farming, cross-league)
(
    'a1b2c3d4-0005-4000-8000-000000000005',
    'tok_font_divine_e9f0a1b2',
    'Font of Divine Skill',
    'Settlers',
    '{
        "type": "leaf",
        "name": "Font of Divine Skill",
        "series": 20,
        "inputs": [{"item": "Skill Gem", "quantity": 3}],
        "outputs": [
            {"item": "Transfigured Gem (RED)", "quantity": 1, "probability": 0.15},
            {"item": "Transfigured Gem (GREEN)", "quantity": 1, "probability": 0.10},
            {"item": "Transfigured Gem (BLUE)", "quantity": 1, "probability": 0.12}
        ],
        "duration_seconds": 600
    }',
    '2026-02-15 20:00:00+00',
    '2026-03-01 11:00:00+00'
);
