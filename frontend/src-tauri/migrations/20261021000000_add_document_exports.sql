-- Documents are derived from committed SQLite content. Never delete external files on cascade.
CREATE TABLE document_exports (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('transcript', 'summary')),
    path TEXT UNIQUE,
    last_hash TEXT,
    pending_hash TEXT,
    last_error TEXT,
    PRIMARY KEY (meeting_id, kind)
);
