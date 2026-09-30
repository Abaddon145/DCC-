use crate::{
    knowledge::{self, Note, NoteInput, NotePage, Notebook, Revision},
    state::AppState,
};
use tauri::State;

#[tauri::command]
pub async fn duplicate_note(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
    expected_revision: i64,
) -> Result<Note, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_mut_if_id(&library_id, |db, base| {
                    crate::note_package::duplicate(db, base, &id, expected_revision)
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn write_note_clipboard_text(text: String) -> Result<(), String> {
    if text.len() > 4 * 1024 * 1024 {
        return Err("剪贴板内容过大".into());
    }
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map_err(|_| "无法写入剪贴板，请使用 Ctrl+C".into())
}

#[tauri::command]
pub fn read_note_markdown(path: String) -> Result<crate::note_markdown::MarkdownSource, String> {
    crate::note_markdown::read(std::path::Path::new(&path))
}
#[tauri::command]
pub async fn import_note_markdown(
    app: tauri::AppHandle,
    library_id: String,
    path: String,
    sha256: String,
    document: serde_json::Value,
    notebook_id: Option<String>,
) -> Result<crate::note_package::ImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_mut_if_id(&library_id, |db, base| {
                    crate::note_markdown::import(
                        db,
                        base,
                        std::path::Path::new(&path),
                        &sha256,
                        document,
                        notebook_id,
                    )
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_note_markdown(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
    revision: i64,
    markdown: String,
    path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_if_id(&library_id, |db, base| {
                    crate::note_markdown::export(
                        db,
                        base,
                        &id,
                        revision,
                        &markdown,
                        std::path::Path::new(&path),
                    )
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn export_note_draft(
    app: tauri::AppHandle,
    library_id: String,
    input: NoteInput,
    path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_if_id(&library_id, |db, base| {
                    crate::note_package::export_draft(db, base, input, std::path::Path::new(&path))
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_note_package(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
    path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_if_id(&library_id, |db, base| {
                    crate::note_package::export(db, base, &id, std::path::Path::new(&path))
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn import_note_package(
    app: tauri::AppHandle,
    library_id: String,
    path: String,
) -> Result<crate::note_package::ImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_mut_if_id(&library_id, |db, base| {
                    crate::note_package::import(db, base, std::path::Path::new(&path))
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn paste_note_clipboard_image(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
) -> Result<crate::knowledge_files::Attachment, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法打开剪贴板")?;
        let image = clipboard
            .get_image()
            .map_err(|_| "剪贴板没有可用图片，请先复制截图")?;
        let staging =
            crate::knowledge_files::stage_clipboard_rgba(image.width, image.height, &image.bytes)?;
        active(state.with_library_if_id(&library_id, |db, base| {
            crate::knowledge_files::import(db, base, &id, &staging.path().join("剪贴板截图.png"))
        })?)
    })
    .await
    .map_err(|e| format!("截图导入任务失败：{e}"))?
}

#[tauri::command]
pub fn search_note_reference_targets(
    state: State<AppState>,
    library_id: String,
    kind: String,
    query: String,
) -> Result<Vec<knowledge::NoteReference>, String> {
    active(state.with_library_if_id(&library_id,|db,_|{
        if query.len()>1000{return Err("搜索词过长".into());}
        let sql=match kind.as_str(){
            "note"=>"SELECT id,title FROM knowledge_notes WHERE deleted_at IS NULL AND title LIKE ?1 ESCAPE '!' ORDER BY updated_at DESC LIMIT 30",
            "asset"=>"SELECT id,name FROM assets WHERE deleted_at IS NULL AND name LIKE ?1 ESCAPE '!' ORDER BY updated_at DESC LIMIT 30",
            "media"=>"SELECT id,name FROM media_entries WHERE deleted_at IS NULL AND name LIKE ?1 ESCAPE '!' ORDER BY updated_at DESC LIMIT 30",
            "project"=>"SELECT id,name FROM projects WHERE name LIKE ?1 ESCAPE '!' ORDER BY updated_at DESC LIMIT 30",
            _=>return Err("未知引用类型".into()),
        };
        let filter=format!("%{}%",query.trim().replace('!',"!!").replace('%',"!%").replace('_',"!_"));
        let mut statement=db.prepare(sql).map_err(|e|e.to_string())?;
        let result=statement.query_map([filter],|row|Ok(knowledge::NoteReference{kind:kind.clone(),target_id:row.get(0)?,label:row.get(1)?,available:true})).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string());result
    })?)
}

#[tauri::command]
pub fn purge_note(
    state: State<AppState>,
    library_id: String,
    id: String,
    expected_revision: i64,
) -> Result<(), String> {
    active(state.with_library_mut_if_id(&library_id, |db, base| {
        crate::knowledge_files::purge(db, base, &id, expected_revision)
    })?)
}
#[tauri::command]
pub fn get_journal_note(
    state: State<AppState>,
    library_id: String,
    date: String,
) -> Result<Option<Note>, String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        use rusqlite::OptionalExtension;
        let id: Option<String> = db
            .query_row(
                "SELECT id FROM knowledge_notes WHERE journal_date=?1",
                [date],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        id.map(|id| knowledge::get(db, &id)).transpose()
    })?)
}

#[tauri::command]
pub fn set_knowledge_editing(state: State<AppState>, editing: bool) {
    state
        .knowledge_editing
        .store(editing, std::sync::atomic::Ordering::SeqCst);
}
#[tauri::command]
pub fn finish_knowledge_close(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    if state.tasks.busy() {
        return Err("请先等待正在执行的任务完成".into());
    }
    state
        .knowledge_editing
        .store(false, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn get_note_references(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<Vec<knowledge::NoteReference>, String> {
    active(state.with_library_if_id(&library_id, |db, _| knowledge::references(db, &id))?)
}
#[tauri::command]
pub fn get_note_backlinks(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<Vec<knowledge::NoteSummary>, String> {
    active(state.with_library_if_id(&library_id, |db, _| knowledge::backlinks(db, &id))?)
}
#[tauri::command]
pub fn delete_empty_notebook(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<(), String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        knowledge::delete_empty_notebook(db, &id)
    })?)
}
#[tauri::command]
pub fn import_note_attachment(
    state: State<AppState>,
    library_id: String,
    id: String,
    path: String,
) -> Result<crate::knowledge_files::Attachment, String> {
    active(state.with_library_if_id(&library_id, |db, base| {
        crate::knowledge_files::import(db, base, &id, std::path::Path::new(&path))
    })?)
}
#[tauri::command]
pub fn open_note_attachment(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<(), String> {
    active(state.with_library_if_id(&library_id, |db, base| {
        let (path, mime) = crate::knowledge_files::path(db, base, &id)?;
        if ![
            "application/pdf",
            "text/plain; charset=utf-8",
            "image/png",
            "image/jpeg",
            "image/gif",
            "image/webp",
        ]
        .contains(&mime.as_str())
        {
            return Err("不允许打开此附件类型".into());
        }
        let extension = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !["pdf", "txt", "md", "png", "jpg", "jpeg", "gif", "webp"].contains(&extension.as_str())
        {
            return Err("附件扩展名不安全".into());
        }
        open::that(path).map_err(|e| e.to_string())
    })?)
}

fn active<T>(value: Option<T>) -> Result<T, String> {
    value.ok_or("素材库已切换，当前笔记草稿未写入其他库".into())
}
#[tauri::command]
pub fn search_notes(
    state: State<AppState>,
    library_id: String,
    query: String,
    notebook_id: Option<String>,
    trash: bool,
    favorite: bool,
    offset: u32,
    limit: u32,
) -> Result<NotePage, String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        knowledge::search(
            db,
            &query,
            notebook_id.as_deref(),
            trash,
            favorite,
            offset,
            limit,
        )
    })?)
}
#[tauri::command]
pub fn get_note(state: State<AppState>, library_id: String, id: String) -> Result<Note, String> {
    active(state.with_library_if_id(&library_id, |db, _| knowledge::get(db, &id))?)
}
#[tauri::command]
pub fn save_note(
    state: State<AppState>,
    library_id: String,
    input: NoteInput,
) -> Result<Note, String> {
    active(state.with_library_mut_if_id(&library_id, |db, _| knowledge::save(db, input))?)
}
#[tauri::command]
pub fn set_note_deleted(
    state: State<AppState>,
    library_id: String,
    id: String,
    expected_revision: i64,
    deleted: bool,
) -> Result<Note, String> {
    active(state.with_library_mut_if_id(&library_id, |db, _| {
        knowledge::set_deleted(db, &id, expected_revision, deleted)
    })?)
}
#[tauri::command]
pub fn list_note_revisions(
    state: State<AppState>,
    library_id: String,
    id: String,
    offset: Option<u32>,
) -> Result<Vec<knowledge::RevisionSummary>, String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        knowledge::revision_summaries(db, &id, offset.unwrap_or(0))
    })?)
}
#[tauri::command]
pub fn get_note_revision(
    state: State<AppState>,
    library_id: String,
    id: String,
    revision_id: String,
) -> Result<Revision, String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        knowledge::revision(db, &id, &revision_id)
    })?)
}
#[tauri::command]
pub fn pin_note_revision(
    state: State<AppState>,
    library_id: String,
    id: String,
    pinned: bool,
) -> Result<(), String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        knowledge::pin_revision(db, &id, pinned)
    })?)
}
#[tauri::command]
pub fn restore_note_revision(
    state: State<AppState>,
    library_id: String,
    id: String,
    revision_id: String,
    expected_revision: i64,
) -> Result<Note, String> {
    active(state.with_library_mut_if_id(&library_id, |db, _| {
        knowledge::restore_revision(db, &id, &revision_id, expected_revision)
    })?)
}
#[tauri::command]
pub fn upsert_notebook(
    state: State<AppState>,
    library_id: String,
    id: Option<String>,
    parent_id: Option<String>,
    name: String,
    sort_order: i64,
) -> Result<Notebook, String> {
    active(state.with_library_mut_if_id(&library_id, |db, _| {
        knowledge::upsert_notebook(db, id.as_deref(), parent_id.as_deref(), &name, sort_order)
    })?)
}
#[tauri::command]
pub fn list_notebooks(state: State<AppState>, library_id: String) -> Result<Vec<Notebook>, String> {
    active(state.with_library_if_id(&library_id,|db,_|{
    let mut statement=db.prepare("SELECT id,parent_id,name,sort_order FROM knowledge_notebooks ORDER BY sort_order,name,id").map_err(|e|e.to_string())?;
    let result=statement.query_map([],|r|Ok(Notebook{id:r.get(0)?,parent_id:r.get(1)?,name:r.get(2)?,sort_order:r.get(3)?})).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string());result
})?)
}
