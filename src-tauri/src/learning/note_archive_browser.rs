//! Read-only source versions; restored archives do not inherit an old write approval.
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveVersion {
    pub id: String,
    pub profile_id: String,
    pub source_key: String,
    pub version_key: String,
    pub sha256: String,
    pub size: u64,
    pub created_at: String,
    pub conversion_state: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePage {
    pub versions: crate::models::Page<ArchiveVersion>,
    pub unconverted_count: i64,
    pub partial_count: i64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveView {
    pub archive_id: String,
    pub sha256: String,
    pub size: u64,
    pub text: Option<String>,
}
pub fn list_archives(db: &Connection, offset: i64, limit: i64) -> Result<ArchivePage, String> {
    let offset = offset.max(0);
    let limit = limit.clamp(1, 50);
    // Aggregate once per query rather than scanning every preview for every archive.
    const VERSIONS: &str = "WITH outcomes AS (
      SELECT archive_id,MAX(CASE WHEN status='committed' THEN
        CASE WHEN json_valid(receipt_json) THEN CASE WHEN json_type(receipt_json,'$.pending')='false' THEN 2 ELSE 1 END ELSE 1 END
        ELSE 0 END) AS state FROM learning_note_previews GROUP BY archive_id
    ), versions AS (
      SELECT a.*,COALESCE(o.state,0) AS state FROM learning_note_archives a LEFT JOIN outcomes o ON o.archive_id=a.id
    ) ";
    let (total, unconverted_count, partial_count) = db.query_row(
        &format!("{VERSIONS} SELECT count(*),COALESCE(sum(state<2),0),COALESCE(sum(state=1),0) FROM versions"),
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).map_err(|e|e.to_string())?;
    let items = db.prepare(&format!("{VERSIONS} SELECT id,source_profile_id,source_key,version_key,sha256,size,created_at,state FROM versions ORDER BY created_at DESC,id DESC LIMIT ?1 OFFSET ?2"))
        .map_err(|e|e.to_string())?.query_map(rusqlite::params![limit,offset],|row| {
            let state:i64=row.get(7)?;
            Ok(ArchiveVersion {id:row.get(0)?,profile_id:row.get(1)?,source_key:row.get(2)?,version_key:row.get(3)?,sha256:row.get(4)?,size:row.get(5)?,created_at:row.get(6)?,conversion_state:match state {2=>"converted",1=>"partial",_=>"unconverted"}.into()})
        }).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    Ok(ArchivePage {
        versions: crate::models::Page {
            items,
            total,
            offset,
            limit,
        },
        unconverted_count,
        partial_count,
    })
}
pub fn read_archive_view(db: &Connection, base: &Path, id: &str) -> Result<ArchiveView, String> {
    use sha2::{Digest, Sha256};
    let bytes = super::note_archive::read_archive(db, base, id)?;
    Ok(ArchiveView {
        archive_id: id.into(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        size: bytes.len() as u64,
        text: String::from_utf8(bytes).ok(),
    })
}
