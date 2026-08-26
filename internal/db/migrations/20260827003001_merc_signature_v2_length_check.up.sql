-- POE-207: the merc icon signature becomes format 2, and format 2 is 1728
-- bytes, not 576.
--
-- Version 1 was a 24x24 grayscale crop of the whole cell: 576 bytes. The gold
-- frame every support cell shares dominated that correlation, so visibly
-- different families scored 0.97-0.99 against each other and no confirmation
-- could ever be trusted. Version 2 keeps only the icon disc and keeps it in
-- COLOUR: 24 x 24 x 3 = 1728 bytes, with the positions outside the disc and
-- inside the tier-badge corner zeroed. See mercenary.SupportedFormatVersion for
-- what version 2 means exactly.
--
-- The CHECK is version-CONDITIONAL, never a flat `octet_length(signature) IN
-- (576, 1728)`. The flat form would accept a 1728-byte row stamped
-- format_version 1, and such a row is served to every device still asking for
-- version 1 — which would decode it as garbage or refuse it, silently, one
-- device at a time. Pairing each length with its version is what makes the
-- column's meaning enforceable rather than conventional.
--
-- THE BRANCH LIST IS CLOSED, AND THAT MAKES IT A DEPLOY-ORDER CONSTRAINT. A
-- format 3 satisfies neither branch, so the day SupportedFormatVersion moves to
-- 3 every upload 500s on this constraint until a migration adds the branch. The
-- migration that adds `format_version = 3 AND octet_length(signature) = <N>`
-- therefore has to SHIP BEFORE the server that writes format-3 rows, not
-- alongside it. This is the same ordering the pool's own deploy note states for
-- the family vocabulary, and it is deliberate: closing the list is what makes a
-- wrong-length row impossible, and the price of that is one extra migration per
-- format bump.
--
-- The same closure applies backwards. ADD CONSTRAINT validates the ENTIRE table
-- and hard-fails if any existing row sits outside {1, 2} — so a database that
-- somehow already holds a format-3 row cannot take this migration at all until
-- that row is dealt with. On this table the set is {1} today, so the ALTER
-- below is a full-table validation that passes; it is not a NOT VALID
-- constraint and must not become one.
--
-- Format-1 rows are LEFT IN PLACE. They are still exactly correct for a client
-- that asks for `?format_version=1`, the serve path filters on the version the
-- client names, and housekeeping of the old corpus is a separate decision from
-- letting the new one in.
--
-- This supersedes the length paragraph in
-- 20260825093000_create_merc_icon_templates.up.sql (the `signature` is the
-- FIRST bytea column... block, lines ~33-40): the "exactly 576 bytes" and
-- "24x24 grayscale signature (SIG_DIM^2)" wording there describes version 1
-- only. Everything else that comment says — why raw GGG colour crops never
-- reach the server, and why the length is a CHECK rather than a comment —
-- holds unchanged for version 2.

ALTER TABLE merc_icon_templates
    DROP CONSTRAINT merc_icon_templates_signature_check;

ALTER TABLE merc_icon_templates
    ADD CONSTRAINT merc_icon_templates_signature_check
    CHECK (
        (format_version = 1 AND octet_length(signature) = 576)
        OR (format_version = 2 AND octet_length(signature) = 1728)
    );
