CREATE TABLE metadata (
    id INTEGER PRIMARY KEY CHECK(id=1),
    wrapped_key BLOB NOT NULL,
    key_check BLOB NOT NULL
) STRICT;
CREATE TABLE drafts (
    id BLOB PRIMARY KEY CHECK(length(id)=16),
    context BLOB NOT NULL CHECK(length(context)=32),
    current_revision INTEGER NOT NULL CHECK(current_revision>0),
    kind TEXT NOT NULL DEFAULT 'draft',
    pinned INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE TABLE preferences(name TEXT PRIMARY KEY, payload BLOB NOT NULL) STRICT;
CREATE INDEX drafts_context ON drafts(context);
CREATE UNIQUE INDEX one_active_draft ON drafts(context) WHERE active=1;
CREATE TABLE revisions (
    draft_id BLOB NOT NULL REFERENCES drafts(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision>0),
    updated_ms INTEGER NOT NULL,
    payload BLOB NOT NULL,
    PRIMARY KEY(draft_id, revision)
) STRICT;
