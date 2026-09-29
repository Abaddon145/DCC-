CREATE TABLE IF NOT EXISTS knowledge_notebooks (
 id TEXT PRIMARY KEY,
 parent_id TEXT REFERENCES knowledge_notebooks(id) ON DELETE RESTRICT,
 name TEXT NOT NULL CHECK(length(trim(name)) > 0),
 sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS knowledge_notes (
 id TEXT PRIMARY KEY,
 notebook_id TEXT REFERENCES knowledge_notebooks(id) ON DELETE RESTRICT,
 title TEXT NOT NULL,
 document_json TEXT NOT NULL,
 plain_text TEXT NOT NULL,
 tags_json TEXT NOT NULL DEFAULT '[]',
 favorite INTEGER NOT NULL DEFAULT 0,
 revision INTEGER NOT NULL DEFAULT 1,
 journal_date TEXT UNIQUE,
 deleted_at TEXT,
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_knowledge_notes_list ON knowledge_notes(deleted_at,notebook_id,updated_at DESC);
CREATE TABLE IF NOT EXISTS knowledge_revisions (
 id TEXT PRIMARY KEY,
 note_id TEXT NOT NULL REFERENCES knowledge_notes(id) ON DELETE CASCADE,
 snapshot_json TEXT NOT NULL,
 pinned INTEGER NOT NULL DEFAULT 0,
 created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_knowledge_revisions_note ON knowledge_revisions(note_id,created_at DESC);
-- Targets deliberately have no foreign keys: deleted sources leave readable reference labels.
CREATE TABLE IF NOT EXISTS knowledge_references (
 note_id TEXT NOT NULL REFERENCES knowledge_notes(id) ON DELETE CASCADE,
 target_kind TEXT NOT NULL CHECK(target_kind IN ('note','asset','media','project')),
 target_id TEXT NOT NULL,
 label TEXT NOT NULL,
 PRIMARY KEY(note_id,target_kind,target_id)
);
CREATE INDEX IF NOT EXISTS idx_knowledge_backlinks ON knowledge_references(target_kind,target_id);
CREATE TABLE IF NOT EXISTS knowledge_attachments (
 id TEXT PRIMARY KEY,
 note_id TEXT NOT NULL REFERENCES knowledge_notes(id) ON DELETE CASCADE,
 filename TEXT NOT NULL,
 relative_path TEXT NOT NULL UNIQUE,
 mime TEXT NOT NULL,
 sha256 TEXT NOT NULL,
 size INTEGER NOT NULL,
 created_at TEXT NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_search USING fts5(note_id UNINDEXED,title,plain_text,tags,tokenize='trigram');
