CREATE TABLE IF NOT EXISTS asset_relations (
  id TEXT PRIMARY KEY,
  source_asset_id TEXT NOT NULL REFERENCES asset_core(id) ON DELETE CASCADE,
  target_asset_id TEXT NOT NULL REFERENCES asset_core(id) ON DELETE CASCADE,
  relation_type TEXT NOT NULL CHECK(relation_type IN ('inspired_by','generated_from','derived_from','used_in','reference_of','variant_of','related_to')),
  created_at TEXT NOT NULL,
  CHECK(source_asset_id <> target_asset_id),
  CHECK(relation_type NOT IN ('variant_of','related_to') OR source_asset_id < target_asset_id),
  UNIQUE(source_asset_id,target_asset_id,relation_type)
);
CREATE INDEX IF NOT EXISTS idx_asset_relations_source ON asset_relations(source_asset_id,target_asset_id);
CREATE INDEX IF NOT EXISTS idx_asset_relations_target ON asset_relations(target_asset_id,source_asset_id);
CREATE TABLE IF NOT EXISTS asset_ai_analysis (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES asset_core(id) ON DELETE CASCADE,
  description TEXT NOT NULL, keywords TEXT NOT NULL CHECK(json_valid(keywords) AND json_type(keywords)='array'),
  scene TEXT NOT NULL, style TEXT NOT NULL, camera TEXT NOT NULL, lighting TEXT NOT NULL, mood TEXT NOT NULL,
  model TEXT NOT NULL, mode TEXT NOT NULL CHECK(mode IN ('text','visual')),
  input_fingerprint TEXT NOT NULL, format_version INTEGER NOT NULL,
  created_at TEXT NOT NULL, actual_tokens INTEGER CHECK(actual_tokens>=0),
  input_snapshot TEXT NOT NULL DEFAULT '' CHECK(input_snapshot='' OR json_valid(input_snapshot))
);
CREATE INDEX IF NOT EXISTS idx_asset_analysis_cache ON asset_ai_analysis(asset_id,input_fingerprint,model,mode,format_version,created_at);
CREATE INDEX IF NOT EXISTS idx_asset_analysis_latest ON asset_ai_analysis(asset_id,created_at);
CREATE TABLE IF NOT EXISTS asset_embeddings (
 asset_id TEXT NOT NULL REFERENCES asset_core(id) ON DELETE CASCADE,
 model TEXT NOT NULL, input_fingerprint TEXT NOT NULL,
 vector TEXT NOT NULL CHECK(json_valid(vector) AND json_type(vector)='array'),
 dimensions INTEGER NOT NULL CHECK(dimensions BETWEEN 1 AND 4096), updated_at TEXT NOT NULL,
 PRIMARY KEY(asset_id,model)
);
CREATE TABLE IF NOT EXISTS asset_entities (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('prompt','shot')),
 name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 200), description TEXT NOT NULL DEFAULT '',
 revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
 deleted_at TEXT, delete_batch_id TEXT REFERENCES deletion_batches(id) ON DELETE SET NULL,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_asset_entities_deleted ON asset_entities(deleted_at,updated_at);
CREATE TABLE IF NOT EXISTS prompts (
 asset_id TEXT PRIMARY KEY REFERENCES asset_core(id) ON DELETE CASCADE,
 content TEXT NOT NULL, model TEXT NOT NULL,
 parameters TEXT NOT NULL CHECK(json_valid(parameters) AND json_type(parameters)='object'),
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS asset_entity_tags (
 entity_id TEXT NOT NULL REFERENCES asset_entities(id) ON DELETE CASCADE,
 tag_id TEXT NOT NULL REFERENCES localized_tags(id) ON DELETE CASCADE, PRIMARY KEY(entity_id,tag_id)
);
CREATE INDEX IF NOT EXISTS idx_asset_entity_tags_tag ON asset_entity_tags(tag_id,entity_id);
CREATE TABLE IF NOT EXISTS asset_entity_projects (
 entity_id TEXT NOT NULL REFERENCES asset_entities(id) ON DELETE CASCADE,
 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, PRIMARY KEY(entity_id,project_id)
);
CREATE INDEX IF NOT EXISTS idx_asset_entity_projects_project ON asset_entity_projects(project_id,entity_id);
CREATE TABLE IF NOT EXISTS shots (
 asset_id TEXT PRIMARY KEY REFERENCES asset_core(id) ON DELETE CASCADE,
 project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
 project_unit_id TEXT UNIQUE REFERENCES project_units(id) ON DELETE SET NULL,
 act TEXT NOT NULL CHECK(length(act)<=80), shot_number TEXT NOT NULL CHECK(length(shot_number)<=80),
 status TEXT NOT NULL CHECK(status IN ('todo','in_progress','review','done')),
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_shots_project ON shots(project_id,created_at,asset_id);
CREATE TRIGGER IF NOT EXISTS shot_unit_sync AFTER UPDATE OF name,description ON project_units
WHEN NEW.name<>OLD.name OR NEW.description<>OLD.description
BEGIN
 UPDATE asset_entities SET name=NEW.name,description=NEW.description,revision=revision+1,updated_at=NEW.updated_at
 WHERE id IN (SELECT c.entity_id FROM asset_core c JOIN shots s ON s.asset_id=c.id WHERE s.project_unit_id=NEW.id);
END;
