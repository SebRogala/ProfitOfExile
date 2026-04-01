CREATE TABLE IF NOT EXISTS lab_layouts (
    difficulty TEXT NOT NULL,
    date DATE NOT NULL,
    layout JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (difficulty, date)
);
