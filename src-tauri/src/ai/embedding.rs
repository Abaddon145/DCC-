use rusqlite::Connection;
pub struct EmbeddingInput {
    pub asset_id: String,
    pub vector: Vec<f32>,
    pub model: String,
    pub input_fingerprint: String,
}
pub fn store(db: &Connection, input: &EmbeddingInput) -> Result<(), String> {
    crate::asset::service::active_origin(db, &input.asset_id)?;
    if input.vector.is_empty()
        || input.vector.len() > 4096
        || input.vector.iter().any(|v| !v.is_finite())
        || input.model.trim().is_empty()
        || input.model.chars().count() > 200
        || input.input_fingerprint.is_empty()
        || input.input_fingerprint.len() > 256
    {
        return Err("向量记录超出限制或包含无效数值".into());
    }
    let vector = serde_json::to_string(&input.vector).map_err(|e| e.to_string())?;
    db.execute("INSERT INTO asset_embeddings(asset_id,model,input_fingerprint,vector,dimensions,updated_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(asset_id,model) DO UPDATE SET input_fingerprint=excluded.input_fingerprint,vector=excluded.vector,dimensions=excluded.dimensions,updated_at=excluded.updated_at",rusqlite::params![input.asset_id,input.model,input.input_fingerprint,vector,input.vector.len(),chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedding_rejects_nonfinite_empty_and_excessive_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let owner = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO assets(id,name,share_url,created_at,updated_at) VALUES(?1,'资产','','now','now')",[&owner]).unwrap();
        let core = crate::asset::repository::register_origin(
            &db,
            &crate::asset::models::CoreOrigin {
                kind: "legacy".into(),
                id: owner,
            },
        )
        .unwrap();
        let mut input = EmbeddingInput {
            asset_id: core.clone(),
            vector: vec![0.1, 0.2],
            model: "local-record".into(),
            input_fingerprint: "input".into(),
        };
        store(&db, &input).unwrap();
        input.vector = vec![0.3, 0.4, 0.5];
        store(&db, &input).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT dimensions FROM asset_embeddings WHERE asset_id=?1",
                [&core],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
        for vector in [vec![], vec![0.0; 4097], vec![f32::NAN], vec![f32::INFINITY]] {
            input.vector = vector;
            assert!(store(&db, &input).is_err());
        }
        input.vector = vec![1.0];
        input.asset_id = uuid::Uuid::new_v4().to_string();
        assert!(store(&db, &input).is_err());
        input.asset_id = core;
        input.model = String::new();
        assert!(store(&db, &input).is_err());
        assert!(!crate::search::semantic::available());
        assert!(crate::search::semantic::search(
            &db,
            &serde_json::from_value(serde_json::json!({})).unwrap()
        )
        .unwrap_err()
        .contains("未配置"));
    }
}
