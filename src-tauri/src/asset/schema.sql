CREATE TABLE IF NOT EXISTS asset_core (
  id TEXT PRIMARY KEY,
  legacy_asset_id TEXT UNIQUE REFERENCES assets(id) ON DELETE CASCADE,
  media_entry_id TEXT UNIQUE REFERENCES media_entries(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  CHECK ((legacy_asset_id IS NULL) <> (media_entry_id IS NULL))
);
CREATE TABLE IF NOT EXISTS asset_files (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES asset_core(id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL CHECK(source_kind IN ('image','assetMedia','mediaFile')),
  source_id TEXT NOT NULL,
  role TEXT NOT NULL CHECK(role IN ('original','dependency','proxy','thumbnail','waveform')),
  filename TEXT NOT NULL,
  extension TEXT NOT NULL,
  size INTEGER NOT NULL DEFAULT 0 CHECK(size>=0),
  hash TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  UNIQUE(asset_id,source_kind,source_id,role),
  UNIQUE(id,asset_id)
);
CREATE INDEX IF NOT EXISTS idx_asset_files_source ON asset_files(source_kind,source_id,role);
CREATE TABLE IF NOT EXISTS asset_metadata (
  file_id TEXT PRIMARY KEY REFERENCES asset_files(id) ON DELETE CASCADE,
  width INTEGER CHECK(width>0), height INTEGER CHECK(height>0),
  duration REAL CHECK(duration>=0), codec TEXT, fps REAL CHECK(fps>0),
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS asset_sources (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES asset_core(id) ON DELETE CASCADE,
  source_type TEXT NOT NULL CHECK(source_type IN ('local','fab','youtube','pinterest','web','other')),
  url TEXT NOT NULL, author TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL,
  UNIQUE(asset_id,source_type,url,author)
);
CREATE TABLE IF NOT EXISTS asset_versions (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES asset_core(id) ON DELETE CASCADE,
  file_id TEXT NOT NULL,
  version TEXT NOT NULL CHECK(length(trim(version)) BETWEEN 1 AND 80),
  note TEXT NOT NULL DEFAULT '' CHECK(length(note)<=10000),
  created_at TEXT NOT NULL,
  UNIQUE(asset_id,version),
  FOREIGN KEY(file_id,asset_id) REFERENCES asset_files(id,asset_id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_asset_versions_file ON asset_versions(file_id);
CREATE TRIGGER IF NOT EXISTS asset_core_version_cleanup BEFORE DELETE ON asset_core
BEGIN DELETE FROM asset_versions WHERE asset_id=OLD.id; END;
CREATE TRIGGER IF NOT EXISTS asset_file_source_insert BEFORE INSERT ON asset_files
WHEN NOT (
 (NEW.source_kind='image' AND NEW.role IN ('original','thumbnail') AND EXISTS(SELECT 1 FROM images WHERE id=NEW.source_id))
 OR (NEW.source_kind='assetMedia' AND EXISTS(SELECT 1 FROM asset_media WHERE id=NEW.source_id AND (NEW.role='original' OR (NEW.role='proxy' AND proxy_rel_path IS NOT NULL) OR (NEW.role='thumbnail' AND thumbnail_rel_path IS NOT NULL))))
 OR (NEW.source_kind='mediaFile' AND EXISTS(SELECT 1 FROM media_files WHERE id=NEW.source_id AND CASE role WHEN 'main' THEN 'original' ELSE role END=NEW.role))
)
BEGIN SELECT RAISE(ABORT,'文件来源未登记或用途不匹配'); END;
CREATE TRIGGER IF NOT EXISTS asset_file_source_immutable BEFORE UPDATE OF source_kind,source_id,role,asset_id ON asset_files
WHEN NEW.source_kind<>OLD.source_kind OR NEW.source_id<>OLD.source_id OR NEW.role<>OLD.role OR NEW.asset_id<>OLD.asset_id
BEGIN SELECT RAISE(ABORT,'文件引用身份不可修改'); END;
CREATE TRIGGER IF NOT EXISTS asset_version_original_insert BEFORE INSERT ON asset_versions
WHEN NOT EXISTS(SELECT 1 FROM asset_files WHERE id=NEW.file_id AND asset_id=NEW.asset_id AND role='original')
BEGIN SELECT RAISE(ABORT,'版本必须引用已登记原件'); END;
CREATE TRIGGER IF NOT EXISTS asset_version_original_update BEFORE UPDATE OF file_id,asset_id ON asset_versions
WHEN NOT EXISTS(SELECT 1 FROM asset_files WHERE id=NEW.file_id AND asset_id=NEW.asset_id AND role='original')
BEGIN SELECT RAISE(ABORT,'版本必须引用已登记原件'); END;
CREATE TRIGGER IF NOT EXISTS asset_image_file_cleanup AFTER DELETE ON images
BEGIN DELETE FROM asset_files WHERE source_kind='image' AND source_id=OLD.id; END;
CREATE TRIGGER IF NOT EXISTS asset_media_file_cleanup AFTER DELETE ON asset_media
BEGIN DELETE FROM asset_files WHERE source_kind='assetMedia' AND source_id=OLD.id; END;
CREATE TRIGGER IF NOT EXISTS asset_library_file_cleanup AFTER DELETE ON media_files
BEGIN DELETE FROM asset_files WHERE source_kind='mediaFile' AND source_id=OLD.id; END;
