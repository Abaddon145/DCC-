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
  created_at TEXT NOT NULL, actual_tokens INTEGER CHECK(actual_tokens>=0)
);
CREATE INDEX IF NOT EXISTS idx_asset_analysis_cache ON asset_ai_analysis(asset_id,input_fingerprint,model,mode,format_version,created_at);
CREATE INDEX IF NOT EXISTS idx_asset_analysis_latest ON asset_ai_analysis(asset_id,created_at);
