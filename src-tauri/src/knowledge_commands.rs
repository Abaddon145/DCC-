use crate::{
    knowledge::{self, Note, NoteInput, NotePage, Notebook, Revision},
    state::AppState,
};
use tauri::State;

#[tauri::command]
pub fn start_note_screenshot() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .arg("ms-screenclip:")
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|_| "无法启动 Windows 区域截屏，请使用 Win+Shift+S 后粘贴".to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    Err("区域截屏仅支持 Windows".into())
}

#[tauri::command]
pub fn get_note_structure_svg(
    state: State<AppState>,
    library_id: String,
    id: String,
    revision: i64,
) -> Result<String, String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        crate::note_export::structure_svg(db, &id, revision)
    })?)
}

#[tauri::command]
pub fn set_note_color(
    state: State<AppState>,
    library_id: String,
    id: String,
    color: Option<String>,
) -> Result<Note, String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        knowledge::set_color(db, &id, color.as_deref())
    })?)
}

#[tauri::command]
pub async fn get_note_export_html(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
    revision: i64,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_if_id(&library_id, |db, base| {
                    crate::note_export::html(db, base, &id, revision)
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_note_map(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
    revision: i64,
    map_id: String,
    format: String,
    png: Option<Vec<u8>>,
    path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_if_id(&library_id, |db, _| {
                    crate::note_export::export_map(
                        db,
                        &id,
                        revision,
                        &map_id,
                        &format,
                        png,
                        std::path::Path::new(&path),
                    )
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn export_note_document(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
    revision: i64,
    format: String,
    path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_if_id(&library_id, |db, base| {
                    crate::note_export::export(
                        db,
                        base,
                        &id,
                        revision,
                        &format,
                        std::path::Path::new(&path),
                    )
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn move_note_item(
    state: State<AppState>,
    library_id: String,
    kind: String,
    id: String,
    parent: Option<String>,
    index: usize,
) -> Result<String, String> {
    active(state.with_library_mut_if_id(&library_id, |db, _| {
        crate::note_moves::move_item(db, &library_id, &kind, &id, parent, index)
    })?)
}
#[tauri::command]
pub fn undo_note_move(
    state: State<AppState>,
    library_id: String,
    token: String,
) -> Result<(), String> {
    active(state.with_library_mut_if_id(&library_id, |db, _| {
        crate::note_moves::undo(db, &library_id, &token)
    })?)
}

#[tauri::command]
pub fn get_note_appearance(state: State<AppState>) -> Result<serde_json::Value, String> {
    state.note_appearance()
}
#[tauri::command]
pub fn save_note_appearance(
    state: State<AppState>,
    value: serde_json::Value,
) -> Result<(), String> {
    state.save_note_appearance(value)
}

#[tauri::command]
pub fn write_note_clipboard_fragment(
    state: State<AppState>,
    library_id: String,
    id: String,
    slice: serde_json::Value,
) -> Result<(), String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        crate::note_clipboard::write(db, &library_id, &id, slice)
    })?)
}
#[tauri::command]
pub async fn read_note_clipboard_fragment(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
) -> Result<Option<serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_mut_if_id(&library_id, |db, base| {
                    crate::note_clipboard::read(db, base, &library_id, &id)
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

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
    use tauri::Manager;
    let valid = app
        .state::<AppState>()
        .with_library_if_id(&library_id, |_, _| Ok(()))?;
    active(valid)?;
    let original = crate::note_markdown::read(std::path::Path::new(&path))?;
    if original.sha256 != sha256 {
        return Err("Markdown 文件已变化，请重新选择".into());
    }
    if document.to_string().len() > 4 * 1024 * 1024 {
        return Err("Markdown 解析结果过大".into());
    }
    let staged = crate::note_remote_images::stage(&document).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let _staging_guard = staged.directory;
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_mut_if_id(&library_id, |db, base| {
                    crate::note_markdown::import_staged(
                        db,
                        base,
                        std::path::Path::new(&path),
                        &sha256,
                        document,
                        notebook_id,
                        &staged.files,
                        &staged.failures,
                    )
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn retry_note_remote_image(
    app: tauri::AppHandle,
    library_id: String,
    id: String,
    expected_revision: i64,
    url: String,
) -> Result<crate::knowledge_files::Attachment, String> {
    use tauri::Manager;
    let document = active(
        app.state::<AppState>()
            .with_library_if_id(&library_id, |db, _| {
                let note = knowledge::get(db, &id)?;
                if note.deleted_at.is_some() || note.revision != expected_revision {
                    return Err("笔记已变化或删除，请保存后重试".into());
                }
                let mut nodes = vec![&note.document];
                let mut matched = false;
                while let Some(node) = nodes.pop() {
                    if node["type"] == "remoteImage" && node["attrs"]["src"] == url {
                        matched = true;
                    }
                    if let Some(children) = node["content"].as_array() {
                        nodes.extend(children);
                    }
                }
                if !matched {
                    return Err("图片不在当前笔记的待重试列表中".into());
                }
                Ok(serde_json::json!({"type":"image","attrs":{"src":url}}))
            })?,
    )?;
    let staged = crate::note_remote_images::stage(&document).await?;
    let source = staged
        .files
        .get(&url)
        .ok_or_else(|| {
            staged
                .failures
                .get(&url)
                .cloned()
                .unwrap_or("图片下载失败".into())
        })?
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _keep = staged;
        active(
            app.state::<AppState>()
                .with_library_if_id(&library_id, |db, base| {
                    let note = knowledge::get(db, &id)?;
                    if note.revision != expected_revision || note.deleted_at.is_some() {
                        return Err("下载期间笔记已变化，请重试".into());
                    }
                    crate::knowledge_files::import(db, base, &id, &source)
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
    manual_order: Option<bool>,
) -> Result<NotePage, String> {
    active(state.with_library_if_id(&library_id, |db, _| {
        knowledge::search_ordered(
            db,
            &query,
            notebook_id.as_deref(),
            trash,
            favorite,
            offset,
            limit,
            manual_order.unwrap_or(false),
        )
    })?)
}
#[tauri::command]
pub async fn reorder_notes(
    app: tauri::AppHandle,
    library_id: String,
    source: String,
    target: String,
    after: bool,
    notebook_id: Option<String>,
    query: String,
    favorite: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        active(
            app.state::<AppState>()
                .with_library_mut_if_id(&library_id, |db, _| {
                    knowledge::reorder_notes(
                        db,
                        &source,
                        &target,
                        after,
                        notebook_id.as_deref(),
                        &query,
                        favorite,
                    )
                })?,
        )
    })
    .await
    .map_err(|e| e.to_string())?
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
