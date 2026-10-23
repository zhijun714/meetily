-- Distinguish user-selected titles from generated titles across regenerations.
ALTER TABLE meetings ADD COLUMN title_manually_edited INTEGER NOT NULL DEFAULT 0;

-- Keep the previous export reachable until the new path is durably written.
ALTER TABLE document_exports ADD COLUMN rename_from TEXT;
ALTER TABLE document_exports ADD COLUMN rename_hash TEXT;
