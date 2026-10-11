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
