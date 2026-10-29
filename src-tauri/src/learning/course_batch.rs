//! Course batch identity is journal metadata, not source authorization.
use super::source_profiles::{self, LibraryScope};
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};

pub(super) fn create_course_batch(
    state: &AppState,
    scope: &LibraryScope,
    profile: &str,
    epoch: &str,
    request_hash: &str,
) -> Result<String, String> {
    if request_hash.len() != 64
        || !request_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("迁入请求摘要无效".into());
    }
    source_profiles::scoped(state, scope, |db| {
        let tx = db.unchecked_transaction().map_err(|_| "无法创建课程批次")?;
        if source_profiles::get_in(&tx, profile)?.access_epoch != epoch {
            return Err("来源状态已变化，请重新检查".into());
        }
        let existing: Option<(String,String,String,String,String)> = tx.query_row("SELECT id,batch_kind,source_profile_id,source_epoch,root_hash FROM learning_import_batches WHERE library_id=?1 AND request_sha256=?2 AND status IN ('pending','committed')", params![scope.library_id(),request_hash], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|_| "无法读取迁入批次")?;
        let id = if let Some((id, kind, old_profile, old_epoch, root)) = existing {
            if kind != "courses"
                || old_profile != profile
                || old_epoch != epoch
                || root != scope.root_hash()
            {
                return Err("迁入请求已被其他批次使用".into());
            }
            id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO learning_import_batches(id,library_id,root_hash,source_profile_id,source_epoch,request_sha256,status,created_at,batch_kind) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7,'courses')", params![id,scope.library_id(),scope.root_hash(),profile,epoch,request_hash,chrono::Utc::now().to_rfc3339()]).map_err(|_| "无法创建课程批次")?;
            id
        };
        tx.commit().map_err(|_| "无法创建课程批次")?;
        Ok(id)
    })
}
