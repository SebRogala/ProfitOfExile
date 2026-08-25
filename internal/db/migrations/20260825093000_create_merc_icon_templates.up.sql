-- POE-200: shared pool of mercenary support-icon templates (POE-165 epic).
--
-- A device that hovers a mercenary's support cell learns the icon's art. This
-- table is where one device's hover becomes every device's template, so the
-- pool bootstraps once instead of once per install.
--
-- NOT league-scoped, and deliberately EXCLUDED from the rollover truncate set
-- (docs/adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md,
-- amended 2026-08-25; docs/LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md). Icon art is
-- league-invariant: the same support gem draws the same picture in Mirage and
-- in the league after it, so wiping this table at rollover would throw away a
-- corpus that is still exactly correct and force every device to relearn 264
-- keys from scratch. Every OTHER new table carrying observations still joins
-- the wipe set at creation — this one is the stated exception, not a
-- precedent for skipping the league column.
--
-- Plain table, not a hypertable: it is not a time series, and every read is
-- "the whole live corpus for one version".
--
-- The corpus that is SERVED is small and bounded — 264 (family, tier) keys x
-- MaxSamplesPerKey (3) = 792 live rows per format version. That ceiling bounds
-- live rows only, not the table. The cap counts `tombstoned_at IS NULL` rows
-- (mercenary.Decide, internal/mercenary/pool.go), a tombstone is an UPDATE that
-- keeps the row so the retired art can still be recognised and refused, and
-- nothing in the server ever issues a DELETE against this table. So a key that
-- is retired and re-learned repeatedly accumulates rows without bound in
-- principle; the practical brakes are the per-device write rate limit and the
-- fact that a retirement is a deliberate user action. If the table ever needs
-- trimming, the row to reap is one tombstoned longer than
-- mercenary.RetiredMatchWindow — past that window it has already stopped
-- voting on new uploads.
--
-- `signature` is the FIRST bytea column in this schema. It holds exactly 576
-- bytes: the 24x24 grayscale signature (SIG_DIM^2) that
-- desktop/src-tauri/src/mercenary/icons.rs derives from a hovered cell, with
-- the tier-badge corner already zeroed. Raw GGG colour crops never reach the
-- server (the repository is GPL because copied art already lives in it; this
-- pool does not add more). The length is a CHECK rather than a comment because
-- a short signature would silently change the correlation's divisor on every
-- device that pulled it.
--
-- `format_version` is part of the key from the first row on purpose. Any change
-- to SIG_DIM, the badge mask, or the luma/normalisation step makes old
-- signatures uncomparable with new ones; without the version in the key such a
-- change would poison every device's matcher at once instead of starting a
-- fresh, empty pool. See mercenary.SupportedFormatVersion for what version 1
-- means.
--
-- `device_id` is provenance only. It is recorded so an abusive uploader can be
-- traced in SQL, and it is NEVER served: the corpus endpoint returns art and
-- keys, never fingerprints (AGENTS.md durable rule).
--
-- `tombstoned_at` is the durable removal path. A device that forgets a bad
-- sample locally would otherwise get it back on the next pull, so a forget
-- becomes a server-side tombstone: the row stops being served, and it is kept
-- rather than deleted so a later upload of the SAME art can be recognised and
-- refused. That is what stops the device that published the bad sample from
-- republishing it before its next pull. What is retired is the art, not the
-- key: better art for the same (family, tier) is still accepted, which is how a
-- family whose key was orphaned by a rename is relearned.

CREATE TABLE merc_icon_templates (
    id             BIGSERIAL   PRIMARY KEY,
    family         TEXT        NOT NULL,
    tier           SMALLINT    NOT NULL CHECK (tier BETWEEN 1 AND 3),
    format_version SMALLINT    NOT NULL CHECK (format_version > 0),
    signature      BYTEA       NOT NULL CHECK (octet_length(signature) = 576),
    device_id      TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    tombstoned_at  TIMESTAMPTZ
);

-- Version first, not family first: every query pins the format version before
-- anything else. The serve path reads one version's whole live corpus, and the
-- upload path reads one version's state for the handful of families in the
-- request. A (family, tier, format_version) order would leave both scanning.
CREATE INDEX idx_merc_icon_templates_version_key
    ON merc_icon_templates (format_version, family, tier);
