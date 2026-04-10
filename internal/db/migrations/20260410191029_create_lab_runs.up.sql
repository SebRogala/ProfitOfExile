CREATE TABLE lab_runs (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL,
    difficulty TEXT NOT NULL CHECK (difficulty IN ('Normal', 'Cruel', 'Merciless', 'Uber')),
    strategy TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    elapsed_seconds INT NOT NULL CHECK (elapsed_seconds > 0 AND elapsed_seconds <= 86400),
    room_count INT NOT NULL CHECK (room_count > 0 AND room_count <= 50),
    has_golden_door BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX idx_lab_runs_device_time ON lab_runs (device_id, started_at DESC);

CREATE TABLE lab_run_rooms (
    id BIGSERIAL PRIMARY KEY,
    run_id BIGINT NOT NULL REFERENCES lab_runs(id) ON DELETE CASCADE,
    room_number INT NOT NULL,
    room_name TEXT NOT NULL,
    entered_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_lab_run_rooms_run ON lab_run_rooms (run_id);
