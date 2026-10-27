-- A project link is independent of the note body; unlinking never rewrites it.
CREATE TABLE IF NOT EXISTS project_learning_notes (
 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
 note_id TEXT NOT NULL REFERENCES knowledge_notes(id) ON DELETE CASCADE,
 created_at TEXT NOT NULL,
 PRIMARY KEY(project_id,note_id)
);
CREATE INDEX IF NOT EXISTS idx_project_learning_notes_note ON project_learning_notes(note_id);
-- Only upgrade once: an existing project reference plus a course link proves association.
INSERT OR IGNORE INTO project_learning_notes(project_id,note_id,created_at)
 SELECT r.target_id,r.note_id,datetime('now')
 FROM knowledge_references r JOIN projects p ON p.id=r.target_id
 WHERE r.target_kind='project'
 AND EXISTS(SELECT 1 FROM learning_course_notes c WHERE c.note_id=r.note_id);
