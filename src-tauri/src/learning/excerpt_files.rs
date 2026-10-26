//! Operation-owned screenshot files. Publishing files alone never commits a note.
use super::excerpt_runtime::ExcerptFrame;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StagedFile {
    pub relative: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Debug, Clone)]
pub struct FrameFiles {
    pub original: StagedFile,
    pub thumbnail: StagedFile,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FileDestination {
    Note { note_id: String },
    Board { board_id: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationFile {
    pub staged_relative: String,
    pub final_relative: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationManifest {
    pub operation_id: String,
    pub excerpt_id: String,
    pub library_id: String,
    pub root_hash: String,
    pub request_sha256: String,
    pub image_id: String,
    pub target: FileDestination,
    pub width: u32,
    pub height: u32,
    pub files: Vec<PublicationFile>,
}
pub fn prepare_files(
    base: &Path,
    operation: &str,
    library: &str,
    root: &str,
    frame: &ExcerptFrame,
) -> Result<FrameFiles, String> {
    uuid(operation)?;
    scope(base, library, root)?;
    let bytes = super::excerpt_store::frame_bytes(frame)?;
    crate::runtime::tools::ensure_space(base, 4 * 1024 * 1024)?;
    let private = tempfile::tempdir().map_err(error)?;
    let input = private.path().join("frame.jpg");
    fs::write(&input, &bytes).map_err(error)?;
    let processed = crate::images::import_image_to(private.path(), &input, "processed")?;
    let stage = |relative: &str, source: &Path| -> Result<StagedFile, String> {
        let (size, sha256) = crate::file_digest::digest_file(source)?;
        if size == 0 || size > 2 * 1024 * 1024 {
            return Err("截图处理文件超限".into());
        }
        let target = destination_path(base, relative, true)?;
        if !target.exists() {
            if let Err(failure) = crate::media::copy_atomic(source, &target) {
                if !target.exists() {
                    return Err(failure);
                }
            }
        }
        check_file(base, relative, size, &sha256)?;
        Ok(StagedFile {
            relative: relative.into(),
            sha256,
            size,
        })
    };
    let prefix = format!("learning-data/excerpts/{operation}/staging");
    let original = stage(
        &format!("{prefix}/frame.jpg"),
        &private.path().join(processed.original_rel_path),
    )?;
    let thumbnail = stage(
        &format!("{prefix}/thumbnail.webp"),
        &private.path().join(processed.thumbnail_rel_path),
    )?;
    scope(base, library, root)?;
    Ok(FrameFiles {
        original,
        thumbnail,
        width: processed.pixel_width,
        height: processed.pixel_height,
    })
}
pub fn create_manifest(
    base: &Path,
    operation: &str,
    excerpt: &str,
    library: &str,
    root: &str,
    request_sha: &str,
    target: FileDestination,
    frame: &FrameFiles,
) -> Result<(PublicationManifest, String), String> {
    let image_id = uuid::Uuid::new_v4().to_string();
    let finals = final_paths(&target, &image_id)?;
    let stages = [&frame.original, &frame.thumbnail];
    let files = finals
        .into_iter()
        .zip(stages)
        .map(|(final_relative, stage)| PublicationFile {
            staged_relative: stage.relative.clone(),
            final_relative,
            sha256: stage.sha256.clone(),
            size: stage.size,
        })
        .collect();
    let manifest = PublicationManifest {
        operation_id: operation.into(),
        excerpt_id: excerpt.into(),
        library_id: library.into(),
        root_hash: root.into(),
        request_sha256: request_sha.into(),
        image_id,
        target,
        width: frame.width,
        height: frame.height,
        files,
    };
    validate(&manifest)?;
    scope(base, library, root)?;
    for file in &manifest.files {
        check_file(base, &file.staged_relative, file.size, &file.sha256)?;
        if destination_path(base, &file.final_relative, false)?.exists() {
            return Err("截图目标已存在，未创建覆盖清单".into());
        }
    }
    let bytes = serde_json::to_vec(&manifest).map_err(error)?;
    if bytes.len() > 8192 {
        return Err("截图清单超限".into());
    }
    let locator = locator(&manifest)?;
    let path = destination_path(base, &locator, true)?;
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("清单目录无效")?).map_err(error)?;
    temp.write_all(&bytes).map_err(error)?;
    temp.as_file().sync_all().map_err(error)?;
    temp.persist_noclobber(path).map_err(error)?;
    Ok((manifest, locator))
}
pub fn read_manifest(
    base: &Path,
    operation: &str,
    excerpt: &str,
    library: &str,
    request_sha: &str,
    location: &str,
) -> Result<PublicationManifest, String> {
    uuid(operation)?;
    uuid(excerpt)?;
    uuid(library)?;
    let prefix = format!("learning-data/excerpts/{operation}/");
    let sha = location
        .strip_prefix(&prefix)
        .and_then(|s| s.strip_suffix(".json"))
        .filter(|s| digest(s))
        .ok_or("截图清单路径无效")?;
    let path = super::files::learning_file(base, location)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(error)?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > 8192 || hash(&bytes) != sha {
        return Err("截图清单超限或摘要不符，未发布或清理文件".into());
    }
    let manifest: PublicationManifest =
        serde_json::from_slice(&bytes).map_err(|_| "截图清单损坏")?;
    validate(&manifest)?;
    scope(base, library, &manifest.root_hash)?;
    if manifest.operation_id != operation
        || manifest.excerpt_id != excerpt
        || manifest.library_id != library
        || manifest.request_sha256 != request_sha
        || locator(&manifest)? != location
    {
        return Err("截图清单不属于此确认操作".into());
    }
    Ok(manifest)
}
pub fn publish_files(base: &Path, manifest: &PublicationManifest) -> Result<(), String> {
    verify_manifest(base, manifest)?;
    // Validate every source/previously copied target before creating any new file.
    for file in &manifest.files {
        check_file(base, &file.staged_relative, file.size, &file.sha256)?;
        if destination_path(base, &file.final_relative, false)?.exists() {
            check_file(base, &file.final_relative, file.size, &file.sha256)?;
        }
    }
    crate::runtime::tools::ensure_space(
        base,
        manifest.files.iter().map(|f| f.size).sum::<u64>() + 1024 * 1024,
    )?;
    for file in &manifest.files {
        let source = check_file(base, &file.staged_relative, file.size, &file.sha256)?;
        let target = destination_path(base, &file.final_relative, true)?;
        if !target.exists() {
            crate::media::copy_atomic(&source, &target)?;
        }
        check_file(base, &file.final_relative, file.size, &file.sha256)?;
    }
    scope(base, &manifest.library_id, &manifest.root_hash)
}
pub fn cleanup_uncommitted(
    db: &Connection,
    base: &Path,
    manifest: &PublicationManifest,
) -> Result<(), String> {
    verify_manifest(base, manifest)?;
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(error)?;
    let (status,excerpt,request,sha,location):(String,String,String,String,Option<String>)=tx.query_row("SELECT status,excerpt_id,request_json,request_sha256,manifest_relative FROM learning_excerpt_operations WHERE operation_id=?1 AND length(CAST(request_json AS BLOB))<=65536",[&manifest.operation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|_|"无法核对操作状态，未清理截图")?;
    if !matches!(status.as_str(), "pending" | "cancelled")
        || excerpt != manifest.excerpt_id
        || sha != manifest.request_sha256
        || hash(request) != sha
        || location != Some(locator(manifest)?)
    {
        return Err("操作已提交或清单归属无法确认，未清理截图".into());
    }
    let mut paths = vec![];
    for file in &manifest.files {
        let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_attachments WHERE relative_path=?1) OR EXISTS(SELECT 1 FROM reference_board_items WHERE original_rel_path=?1 OR thumbnail_rel_path=?1)",[&file.final_relative],|r|r.get(0)).map_err(error)?;
        if used {
            return Err("截图已被笔记或参考板登记，未清理".into());
        }
        if destination_path(base, &file.final_relative, false)?.exists() {
            paths.push(check_file(
                base,
                &file.final_relative,
                file.size,
                &file.sha256,
            )?);
        }
    }
    // No recursive deletion, and never clean material based on an unreadable DB.
    for path in paths {
        crate::media_access::reject_links(&path)?;
        fs::remove_file(path).map_err(error)?;
    }
    tx.commit().map_err(error)
}

fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn digest(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id) {
        Ok(())
    } else {
        Err("截图操作或目标身份无效".into())
    }
}
fn scope(base: &Path, library: &str, root: &str) -> Result<(), String> {
    uuid(library)?;
    if !digest(root)
        || super::analysis_store::library_identity(base)? != (library.to_owned(), root.to_owned())
    {
        return Err("截图不属于当前素材库位置".into());
    }
    Ok(())
}
fn final_paths(target: &FileDestination, image: &str) -> Result<Vec<String>, String> {
    uuid(image)?;
    Ok(match target {
        FileDestination::Note { note_id } => {
            uuid(note_id)?;
            vec![format!("note-attachments/{note_id}/{image}.jpg")]
        }
        FileDestination::Board { board_id } => {
            uuid(board_id)?;
            vec![
                format!("reference-boards/{board_id}/originals/{image}.jpg"),
                format!("reference-boards/{board_id}/thumbnails/{image}.webp"),
            ]
        }
    })
}
fn validate(manifest: &PublicationManifest) -> Result<(), String> {
    uuid(&manifest.operation_id)?;
    uuid(&manifest.excerpt_id)?;
    uuid(&manifest.library_id)?;
    let finals = final_paths(&manifest.target, &manifest.image_id)?;
    if !digest(&manifest.root_hash)
        || !digest(&manifest.request_sha256)
        || manifest.width == 0
        || manifest.height == 0
        || manifest.width > 1024
        || manifest.height > 1024
        || manifest.files.len() != finals.len()
    {
        return Err("截图清单身份、尺寸或文件数量无效".into());
    }
    let stages = [
        format!(
            "learning-data/excerpts/{}/staging/frame.jpg",
            manifest.operation_id
        ),
        format!(
            "learning-data/excerpts/{}/staging/thumbnail.webp",
            manifest.operation_id
        ),
    ];
    for (index, (file, path)) in manifest.files.iter().zip(finals).enumerate() {
        if file.final_relative != path
            || file.staged_relative != stages[index]
            || !digest(&file.sha256)
            || file.size == 0
            || file.size > 2 * 1024 * 1024
            || (index == 0 && file.size > 256 * 1024)
        {
            return Err("截图清单文件路径、摘要或大小无效".into());
        }
    }
    Ok(())
}
pub(super) fn locator(manifest: &PublicationManifest) -> Result<String, String> {
    validate(manifest)?;
    Ok(format!(
        "learning-data/excerpts/{}/{}.json",
        manifest.operation_id,
        hash(serde_json::to_vec(manifest).map_err(error)?)
    ))
}
fn verify_manifest(base: &Path, manifest: &PublicationManifest) -> Result<(), String> {
    read_manifest(
        base,
        &manifest.operation_id,
        &manifest.excerpt_id,
        &manifest.library_id,
        &manifest.request_sha256,
        &locator(manifest)?,
    )?;
    Ok(())
}
fn destination_path(base: &Path, relative: &str, create: bool) -> Result<PathBuf, String> {
    crate::media_access::reject_links(base)?;
    let path = crate::media::safe_relative(base, relative)?;
    crate::media_access::reject_links(&path)?;
    let parent = path.parent().ok_or("截图目录无效")?;
    if create {
        fs::create_dir_all(parent).map_err(error)?;
        crate::media_access::reject_links(&path)?;
    }
    if parent.exists()
        && !parent
            .canonicalize()
            .map_err(error)?
            .starts_with(base.canonicalize().map_err(error)?)
    {
        return Err("截图目录不在当前库内".into());
    }
    Ok(path)
}
fn check_file(base: &Path, relative: &str, size: u64, sha: &str) -> Result<PathBuf, String> {
    let path = destination_path(base, relative, false)?;
    let metadata = fs::metadata(&path).map_err(|_| "截图文件缺失")?;
    if !metadata.is_file() {
        return Err("截图文件缺失".into());
    }
    if size > 2 * 1024 * 1024 || metadata.len() != size {
        return Err("截图文件大小已改变或超限，未读取大文件".into());
    }
    // A changed stage cannot turn a small screenshot check into an unbounded media read.
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(error)?
        .take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as u64 != size || hash(&bytes) != sha {
        return Err("截图文件摘要或大小已改变，未覆盖或清理".into());
    }
    Ok(path)
}
