-- Restores the format-1-only length CHECK.
--
-- THIS DOWN MIGRATION FAILS if any format-2 row exists: the restored constraint
-- is validated against the whole table, and a 1728-byte signature does not
-- satisfy `octet_length(signature) = 576`. That failure is the correct
-- behaviour and must not be worked around with NOT VALID — the rollback target
-- is a schema whose only legal signature is 576 bytes, and data that
-- contradicts it has to be dealt with deliberately. To roll back a server that
-- has already accepted format-2 uploads, delete or archive the
-- `format_version = 2` rows first; a redeploy of the new server re-accepts
-- them from the devices that still hold them.

ALTER TABLE merc_icon_templates
    DROP CONSTRAINT merc_icon_templates_signature_check;

ALTER TABLE merc_icon_templates
    ADD CONSTRAINT merc_icon_templates_signature_check
    CHECK (octet_length(signature) = 576);
