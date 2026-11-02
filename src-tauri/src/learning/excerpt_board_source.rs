//! Resolve a reference-board image only through its committed local receipt.
use super::{
    excerpt_publish::{CommitRequest, Destination, ExcerptReceipt},
    note_nodes::{LearningReference, TimeRange},
};
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardJump {
    pub course_id: String,
    pub media_id: String,
    pub position_ms: u64,
    pub end_ms: u64,
    pub source_fingerprint: String,
}
#[derive(Debug, PartialEq)]
struct Snapshot {
    reference: LearningReference,
    excerpt_id: String,
    operation_id: String,
    request: CommitRequest,
    receipt: ExcerptReceipt,
    payload_sha: String,
    root_hash: String,
}
fn uuid(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id) {
        Ok(())
    } else {
        Err("参考板或图片ID无效".into())
    }
}

fn snapshot(
    state: &AppState,
    library: &str,
    board: &str,
    item: &str,
) -> Result<Option<Snapshot>, String> {
    uuid(board)?;
    uuid(item)?;
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护，请稍后打开来源".into());
    }
    super::excerpt_access::scoped(state, library, |db, base, _| {
        let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
        let row:Option<(Option<String>,Option<String>)>=tx.query_row("SELECT m.excerpt_id,o.operation_id FROM reference_board_items i LEFT JOIN learning_excerpt_board_items m ON m.item_id=i.id LEFT JOIN learning_excerpt_operations o ON o.excerpt_id=m.excerpt_id WHERE i.id=?1 AND i.board_id=?2 AND i.deleted_at IS NULL",params![item,board],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        let (excerpt, operation) = match row {
            None => return Err("参考图不存在或已删除".into()),
            Some((None, None)) => return Ok(None),
            Some((Some(e), Some(o))) => (e, o),
            _ => return Err("参考图的学习来源记录不完整".into()),
        };
        let record = super::excerpt_access::read_record(&tx, &operation)?;
        let receipt = record.receipt.clone().ok_or("学习来源尚未保存")?;
        if record.status != "committed"
            || record.excerpt_id != excerpt
            || !record.request.include_frame
            || !matches!(&record.request.destination,Destination::Board{board_id,..} if board_id==board)
            || receipt.board_id.as_deref() != Some(board)
            || receipt.board_item_id.as_deref() != Some(item)
        {
            return Err("参考图与学习来源回执不一致".into());
        }
        let payload_sha: String = tx
            .query_row(
                "SELECT payload_sha256 FROM learning_excerpts WHERE id=?1",
                [&excerpt],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let stored = super::excerpt_store::get_preview(&tx, base, library, &excerpt)?;
        let p = stored.prepared;
        if stored.status != "committed" || p.library_id != library {
            return Err("学习来源不属于当前素材库".into());
        }
        let at = p.frame.ok_or("学习截图记录缺少帧")?.time_ms;
        let reference = LearningReference {
            course_id: p.text.course_id,
            media_id: p.text.media_id,
            fingerprint: p.source_sha256,
            ranges: vec![TimeRange {
                start_ms: at,
                end_ms: at.checked_add(1).ok_or("截图时间无效")?,
            }],
            label: p.text.title,
        };
        reference.validate()?;
        let root_hash = super::analysis_store::library_identity(base)?.1;
        let result = Snapshot {
            reference,
            excerpt_id: excerpt,
            operation_id: operation,
            request: record.request,
            receipt,
            payload_sha,
            root_hash,
        };
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Some(result))
    })
}
pub fn get_in_library(
    state: &AppState,
    library: &str,
    board: &str,
    item: &str,
) -> Result<Option<LearningReference>, String> {
    Ok(snapshot(state, library, board, item)?.map(|s| s.reference))
}
pub(super) fn prepare_guarded(
    state: &AppState,
    library: &str,
    board: &str,
    item: &str,
    after: impl FnOnce() -> Result<(), String>,
) -> Result<Option<BoardJump>, String> {
    let Some(first) = snapshot(state, library, board, item)? else {
        return Ok(None);
    };
    let jump = super::note_nodes::prepare_jump(state, library, &first.reference, 0)?;
    after()?;
    if snapshot(state, library, board, item)?.as_ref() != Some(&first) {
        return Err("参考图或学习来源已变化，请重新选择".into());
    }
    Ok(Some(BoardJump {
        course_id: jump.course_id,
        media_id: jump.media_id,
        position_ms: jump.position_ms,
        end_ms: jump.end_ms,
        source_fingerprint: first.reference.fingerprint,
    }))
}
pub fn prepare_in_library(
    state: &AppState,
    library: &str,
    board: &str,
    item: &str,
) -> Result<Option<BoardJump>, String> {
    prepare_guarded(state, library, board, item, || Ok(()))
}
#[tauri::command]
pub async fn prepare_learning_board_excerpt_jump(
    app: tauri::AppHandle,
    expected_library_id: String,
    board_id: String,
    item_id: String,
) -> Result<Option<BoardJump>, String> {
    super::excerpt_access::job(app, expected_library_id, move |state, library| {
        prepare_in_library(state, library, &board_id, &item_id)
    })
    .await
}
