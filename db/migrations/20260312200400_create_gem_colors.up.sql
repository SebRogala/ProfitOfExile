CREATE TABLE gem_colors (
    name   TEXT PRIMARY KEY,
    color  TEXT NOT NULL CHECK (color IN ('RED', 'GREEN', 'BLUE', 'WHITE'))
);
