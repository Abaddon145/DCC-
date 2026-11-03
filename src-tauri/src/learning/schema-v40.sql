CREATE TABLE IF NOT EXISTS media_workspace_memberships (
 media_entry_id TEXT NOT NULL REFERENCES media_entries(id) ON DELETE CASCADE,
 workspace TEXT NOT NULL CHECK(workspace IN ('materials','learning')),
 PRIMARY KEY(media_entry_id,workspace)
);
CREATE TABLE IF NOT EXISTS media_workspace_trash (
 batch_id TEXT NOT NULL REFERENCES deletion_batches(id) ON DELETE CASCADE,
 media_entry_id TEXT NOT NULL REFERENCES media_entries(id) ON DELETE CASCADE,
 workspace TEXT NOT NULL CHECK(workspace IN ('materials','learning')),
 PRIMARY KEY(batch_id,media_entry_id,workspace)
);
CREATE VIEW IF NOT EXISTS media_workspace_effective_memberships AS
 SELECT media_entry_id,workspace FROM media_workspace_memberships
 UNION SELECT media_entry_id,'learning' FROM learning_courses;
INSERT OR IGNORE INTO media_workspace_memberships(media_entry_id,workspace)
 SELECT id,'materials' FROM media_entries;
INSERT OR IGNORE INTO media_workspace_memberships(media_entry_id,workspace)
 SELECT media_entry_id,'learning' FROM learning_courses;
CREATE TRIGGER IF NOT EXISTS media_workspace_default_materials
AFTER INSERT ON media_entries
BEGIN
 INSERT INTO media_workspace_memberships(media_entry_id,workspace)
 VALUES(NEW.id,'materials');
END;
CREATE TRIGGER IF NOT EXISTS media_workspace_protect_learning_trash
BEFORE UPDATE OF deleted_at ON media_entries
WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL AND
 EXISTS(SELECT 1 FROM media_workspace_effective_memberships w WHERE w.media_entry_id=OLD.id AND w.workspace='learning')
BEGIN
 SELECT RAISE(ABORT,'媒体仍被学习库使用，无法删除');
END;
CREATE TRIGGER IF NOT EXISTS media_workspace_protect_learning_delete
BEFORE DELETE ON media_entries
WHEN EXISTS(SELECT 1 FROM media_workspace_effective_memberships w WHERE w.media_entry_id=OLD.id AND w.workspace='learning')
BEGIN
 SELECT RAISE(ABORT,'媒体仍被学习库使用，无法删除');
END;
