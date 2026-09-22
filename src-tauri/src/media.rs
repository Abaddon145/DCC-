use crate::models::AssetMedia;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mov", "mkv"];
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac"];
const MODEL_EXTENSIONS: &[&str] = &["glb", "gltf", "fbx", "obj", "stl"];

fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn media_kind(path: &Path) -> Result<(&'static str, &'static str), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let value = extension.as_str();
    if VIDEO_EXTENSIONS.contains(&value) {
        Ok((
            "video",
            match value {
                "mp4" => "video/mp4",
                "webm" => "video/webm",
                "mov" => "video/quicktime",
                _ => "video/x-matroska",
            },
        ))
    } else if AUDIO_EXTENSIONS.contains(&value) {
        Ok((
            "audio",
            match value {
                "mp3" => "audio/mpeg",
                "wav" => "audio/wav",
                "ogg" => "audio/ogg",
                _ => "audio/flac",
            },
        ))
    } else if MODEL_EXTENSIONS.contains(&value) {
        Ok((
            "model",
            match value {
                "glb" => "model/gltf-binary",
                "gltf" => "model/gltf+json",
                "fbx" => "application/vnd.autodesk.fbx",
                "obj" => "model/obj",
                _ => "model/stl",
            },
        ))
    } else {
        Err("仅支持 MP4、WebM、MOV、MKV、MP3、WAV、OGG、FLAC、GLB、GLTF、FBX、OBJ 和 STL".into())
    }
}

pub(crate) fn safe_relative(base_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("媒体路径不安全".into());
    }
    Ok(base_dir.join(relative))
}

pub(crate) fn copy_atomic(source: &Path, target: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or("媒体目标目录无效")?;
    fs::create_dir_all(parent).map_err(|error| format!("创建媒体目录失败：{error}"))?;
    let temp = target.with_extension(format!(
        "{}.partial",
        target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("tmp")
    ));
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    fs::copy(source, &temp).map_err(|error| format!("复制媒体失败：{error}"))?;
    fs::rename(&temp, target).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("保存媒体失败：{error}")
    })
}

#[cfg(windows)]
fn quiet(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
}
#[cfg(not(windows))]
fn quiet(_: &mut Command) {}

pub fn ffmpeg_paths(resource_dir: Option<&Path>) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut roots = Vec::new();
    if let Some(root) = resource_dir {
        roots.push(root.join("media-tools"));
        roots.push(root.join("resources").join("media-tools"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            roots.push(root.join("media-tools"));
            roots.push(root.to_path_buf());
        }
    }
    for root in roots {
        let ffmpeg = root.join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        let ffprobe = root.join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        });
        if ffmpeg.is_file() && ffprobe.is_file() {
            return (Some(ffmpeg), Some(ffprobe));
        }
    }
    (None, None)
}

fn run(command: &mut Command) -> Result<(), String> {
    quiet(command);
    let output = command
        .output()
        .map_err(|error| format!("启动媒体引擎失败：{error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let message = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "媒体处理失败：{}",
            message
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("未知错误")
        ))
    }
}

pub(crate) fn probe(
    ffprobe: Option<&Path>,
    input: &Path,
) -> (Option<i64>, Option<i64>, Option<i64>) {
    let Some(ffprobe) = ffprobe else {
        return (None, None, None);
    };
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input);
    quiet(&mut command);
    let Ok(output) = command.output() else {
        return (None, None, None);
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return (None, None, None);
    };
    let duration = value
        .get("format")
        .and_then(|v| v.get("duration"))
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| (v * 1000.0).round() as i64);
    let video = value
        .get("streams")
        .and_then(|v| v.as_array())
        .and_then(|values| {
            values
                .iter()
                .find(|item| item.get("codec_type").and_then(|v| v.as_str()) == Some("video"))
        });
    let width = video.and_then(|v| v.get("width")).and_then(|v| v.as_i64());
    let height = video.and_then(|v| v.get("height")).and_then(|v| v.as_i64());
    (duration, width, height)
}

pub(crate) fn process(
    ffmpeg: Option<&Path>,
    kind: &str,
    extension: &str,
    original: &Path,
    proxy: &Path,
    thumbnail: &Path,
) -> Result<(Option<String>, Option<String>, String), String> {
    if kind == "model" {
        return Ok((None, None, "ready".into()));
    }
    let Some(ffmpeg) = ffmpeg else {
        let native = matches!(extension, "mp4" | "webm" | "mp3" | "wav" | "ogg" | "flac");
        return if native {
            Ok((None, None, "ready".into()))
        } else {
            Err("未找到内置 FFmpeg，无法处理该格式".into())
        };
    };
    fs::create_dir_all(proxy.parent().ok_or("代理目录无效")?).map_err(|error| error.to_string())?;
    fs::create_dir_all(thumbnail.parent().ok_or("缩略图目录无效")?)
        .map_err(|error| error.to_string())?;
    if kind == "video" {
        let mut thumb = Command::new(ffmpeg);
        thumb
            .args(["-y", "-ss", "1", "-i"])
            .arg(original)
            .args(["-frames:v", "1", "-vf", "scale=640:-2"])
            .arg(thumbnail);
        run(&mut thumb)?;
        if matches!(extension, "mov" | "mkv") {
            let mut transcode = Command::new(ffmpeg);
            transcode
                .args(["-y", "-i"])
                .arg(original)
                .args([
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    "34",
                    "-b:v",
                    "0",
                    "-c:a",
                    "libopus",
                    "-deadline",
                    "good",
                ])
                .arg(proxy);
            run(&mut transcode)?;
            return Ok((
                Some(proxy.to_string_lossy().into_owned()),
                Some(thumbnail.to_string_lossy().into_owned()),
                "ready".into(),
            ));
        }
        Ok((
            None,
            Some(thumbnail.to_string_lossy().into_owned()),
            "ready".into(),
        ))
    } else {
        let mut waveform = Command::new(ffmpeg);
        waveform
            .args(["-y", "-i"])
            .arg(original)
            .args([
                "-filter_complex",
                "showwavespic=s=800x180:colors=#d99a42",
                "-frames:v",
                "1",
            ])
            .arg(thumbnail);
        run(&mut waveform)?;
        if extension == "flac" {
            let mut transcode = Command::new(ffmpeg);
            transcode
                .args(["-y", "-i"])
                .arg(original)
                .args(["-c:a", "libopus", "-b:a", "160k"])
                .arg(proxy);
            run(&mut transcode)?;
            return Ok((
                Some(proxy.to_string_lossy().into_owned()),
                Some(thumbnail.to_string_lossy().into_owned()),
                "ready".into(),
            ));
        }
        Ok((
            None,
            Some(thumbnail.to_string_lossy().into_owned()),
            "ready".into(),
        ))
    }
}

fn row_media(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetMedia> {
    Ok(AssetMedia {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        kind: row.get(2)?,
        original_name: row.get(3)?,
        mime_type: row.get(4)?,
        file_size: row.get(5)?,
        duration_ms: row.get(6)?,
        pixel_width: row.get(7)?,
        pixel_height: row.get(8)?,
        sort_order: row.get(9)?,
        is_cover: row.get::<_, i64>(10)? != 0,
        processing_status: row.get(11)?,
        processing_message: row.get(12)?,
        has_proxy: row.get::<_, i64>(13)? != 0,
        has_thumbnail: row.get::<_, i64>(14)? != 0,
    })
}

pub fn list(connection: &Connection, asset_id: &str) -> Result<Vec<AssetMedia>, String> {
    let mut statement = connection.prepare("SELECT id,asset_id,kind,original_name,mime_type,file_size,duration_ms,pixel_width,pixel_height,sort_order,is_cover,processing_status,processing_message,proxy_rel_path IS NOT NULL,thumbnail_rel_path IS NOT NULL FROM asset_media WHERE asset_id=?1 ORDER BY sort_order,id").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([asset_id], row_media)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn import(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    asset_id: &str,
    paths: Vec<String>,
) -> Result<Vec<AssetMedia>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    if paths.len() > 100 {
        return Err("一次最多导入 100 个预览附件".into());
    }
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",
            [asset_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        != 0;
    if !exists {
        return Err("素材不存在".into());
    }
    let (ffmpeg, ffprobe) = ffmpeg_paths(resource_dir);
    let mut order: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(sort_order),-1)+1 FROM asset_media WHERE asset_id=?1",
            [asset_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let has_cover = connection.query_row("SELECT EXISTS(SELECT 1 FROM images WHERE asset_id=?1 AND is_cover=1) OR EXISTS(SELECT 1 FROM asset_media WHERE asset_id=?1 AND is_cover=1)", [asset_id], |row| row.get::<_,i64>(0)).map_err(|error| error.to_string())? != 0;
    let mut imported_ids = Vec::new();
    for (position, raw) in paths.into_iter().enumerate() {
        let source = PathBuf::from(raw);
        if !source.is_file() {
            return Err(format!("预览附件不存在：{}", source.display()));
        }
        let (kind, mime) = media_kind(&source)?;
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let id = Uuid::new_v4().to_string();
        let original_rel = format!("media/{asset_id}/originals/{id}.{extension}");
        let proxy_ext = if kind == "video" { "webm" } else { "ogg" };
        let proxy_rel = format!("media/{asset_id}/proxies/{id}.{proxy_ext}");
        let thumb_rel = format!("media/{asset_id}/thumbnails/{id}.png");
        let original = safe_relative(base_dir, &original_rel)?;
        let proxy = safe_relative(base_dir, &proxy_rel)?;
        let thumb = safe_relative(base_dir, &thumb_rel)?;
        copy_atomic(&source, &original)?;
        let (duration, width, height) = probe(ffprobe.as_deref(), &original);
        let processed = process(
            ffmpeg.as_deref(),
            kind,
            &extension,
            &original,
            &proxy,
            &thumb,
        );
        let (proxy_value, thumb_value, status, message) = match processed {
            Ok((proxy_path, thumb_path, status)) => (
                proxy_path.map(|_| proxy_rel.clone()),
                thumb_path.map(|_| thumb_rel.clone()),
                status,
                String::new(),
            ),
            Err(error) => (None, None, "error".into(), error),
        };
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("media")
            .chars()
            .take(255)
            .collect::<String>();
        let size = fs::metadata(&original)
            .map(|value| value.len() as i64)
            .unwrap_or(0);
        let timestamp = now();
        let is_cover = !has_cover && position == 0;
        if let Err(error) = connection.execute("INSERT INTO asset_media(id,asset_id,kind,original_name,original_rel_path,proxy_rel_path,thumbnail_rel_path,mime_type,file_size,duration_ms,pixel_width,pixel_height,sort_order,is_cover,processing_status,processing_message,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17)", params![id,asset_id,kind,name,original_rel,proxy_value,thumb_value,mime,size,duration,width,height,order,is_cover as i64,status,message,timestamp]) {
            let _=fs::remove_file(&original); let _=fs::remove_file(&proxy); let _=fs::remove_file(&thumb); return Err(error.to_string());
        }
        imported_ids.push(id);
        order += 1;
    }
    list(connection, asset_id).map(|values| {
        values
            .into_iter()
            .filter(|value| imported_ids.contains(&value.id))
            .collect()
    })
}

pub fn delete(connection: &mut Connection, base_dir: &Path, id: &str) -> Result<(), String> {
    let paths: (String,Option<String>,Option<String>,String) = connection.query_row("SELECT original_rel_path,proxy_rel_path,thumbnail_rel_path,asset_id FROM asset_media WHERE id=?1", [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(|error| error.to_string())?.ok_or("预览附件不存在")?;
    connection
        .execute("DELETE FROM asset_media WHERE id=?1", [id])
        .map_err(|error| error.to_string())?;
    for relative in [Some(paths.0), paths.1, paths.2].into_iter().flatten() {
        if let Ok(path) = safe_relative(base_dir, &relative) {
            let _ = fs::remove_file(path);
        }
    }
    normalize_cover(connection, &paths.3)
}

pub fn retry(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    id: &str,
) -> Result<AssetMedia, String> {
    let (asset_id, kind, original_rel): (String, String, String) = connection
        .query_row(
            "SELECT asset_id,kind,original_rel_path FROM asset_media WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or("预览附件不存在")?;
    let original = safe_relative(base_dir, &original_rel)?;
    if !original.is_file() {
        return Err("预览附件原件缺失，无法重试".into());
    }
    let extension = original
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let proxy_rel = format!(
        "media/{asset_id}/proxies/{id}.{}",
        if kind == "video" { "webm" } else { "ogg" }
    );
    let thumbnail_rel = format!("media/{asset_id}/thumbnails/{id}.png");
    let proxy = safe_relative(base_dir, &proxy_rel)?;
    let thumbnail = safe_relative(base_dir, &thumbnail_rel)?;
    connection.execute("UPDATE asset_media SET processing_status='processing',processing_message='',updated_at=?1 WHERE id=?2", params![now(),id]).map_err(|error| error.to_string())?;
    let (ffmpeg, _) = ffmpeg_paths(resource_dir);
    match process(
        ffmpeg.as_deref(),
        &kind,
        &extension,
        &original,
        &proxy,
        &thumbnail,
    ) {
        Ok((proxy_path, thumbnail_path, status)) => {
            connection.execute("UPDATE asset_media SET proxy_rel_path=?1,thumbnail_rel_path=?2,processing_status=?3,processing_message='',updated_at=?4 WHERE id=?5", params![proxy_path.map(|_| proxy_rel),thumbnail_path.map(|_| thumbnail_rel),status,now(),id]).map_err(|error| error.to_string())?;
        }
        Err(error) => {
            connection.execute("UPDATE asset_media SET processing_status='error',processing_message=?1,updated_at=?2 WHERE id=?3", params![error,now(),id]).map_err(|value| value.to_string())?;
            return Err(error);
        }
    }
    list(connection, &asset_id)?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "读取附件失败".into())
}

pub fn reorder(
    connection: &mut Connection,
    asset_id: &str,
    ids: Vec<String>,
) -> Result<(), String> {
    let current = list(connection, asset_id)?
        .into_iter()
        .map(|value| value.id)
        .collect::<Vec<_>>();
    if current.len() != ids.len()
        || current.iter().collect::<std::collections::HashSet<_>>()
            != ids.iter().collect::<std::collections::HashSet<_>>()
    {
        return Err("排序列表必须包含全部媒体附件".into());
    }
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    for (index, id) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE asset_media SET sort_order=?1,updated_at=?2 WHERE id=?3 AND asset_id=?4",
                params![index as i64, now(), id, asset_id],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn set_cover(
    connection: &mut Connection,
    asset_id: &str,
    media_id: Option<&str>,
    image_id: Option<&str>,
) -> Result<(), String> {
    if media_id.is_some() == image_id.is_some() {
        return Err("必须且只能选择一个封面".into());
    }
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute("UPDATE images SET is_cover=0 WHERE asset_id=?1", [asset_id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE asset_media SET is_cover=0 WHERE asset_id=?1",
            [asset_id],
        )
        .map_err(|error| error.to_string())?;
    let changed = if let Some(id) = media_id {
        transaction.execute(
            "UPDATE asset_media SET is_cover=1 WHERE id=?1 AND asset_id=?2",
            params![id, asset_id],
        )
    } else {
        transaction.execute(
            "UPDATE images SET is_cover=1 WHERE id=?1 AND asset_id=?2",
            params![image_id, asset_id],
        )
    }
    .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("封面附件不存在".into());
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn normalize_cover(connection: &Connection, asset_id: &str) -> Result<(), String> {
    let any = connection.query_row("SELECT EXISTS(SELECT 1 FROM images WHERE asset_id=?1 AND is_cover=1) OR EXISTS(SELECT 1 FROM asset_media WHERE asset_id=?1 AND is_cover=1)", [asset_id], |row| row.get::<_,i64>(0)).map_err(|error| error.to_string())? != 0;
    if any {
        return Ok(());
    }
    if connection.execute("UPDATE images SET is_cover=1 WHERE id=(SELECT id FROM images WHERE asset_id=?1 ORDER BY sort_order LIMIT 1)", [asset_id]).map_err(|error| error.to_string())? == 0 {
        connection.execute("UPDATE asset_media SET is_cover=1 WHERE id=(SELECT id FROM asset_media WHERE asset_id=?1 ORDER BY sort_order LIMIT 1)", [asset_id]).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn media_path(
    connection: &Connection,
    base_dir: &Path,
    id: &str,
    variant: &str,
) -> Result<(PathBuf, String), String> {
    let (original,proxy,thumbnail,mime,kind): (String,Option<String>,Option<String>,String,String) = connection.query_row("SELECT original_rel_path,proxy_rel_path,thumbnail_rel_path,mime_type,kind FROM asset_media WHERE id=?1", [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional().map_err(|error| error.to_string())?.ok_or("预览附件不存在")?;
    let (relative, content_type) = match variant {
        "thumbnail" => (thumbnail.ok_or("该附件没有缩略图")?, "image/png".into()),
        "original" => (original, mime),
        _ => match proxy {
            Some(proxy) => (
                proxy,
                if kind == "video" {
                    "video/webm".into()
                } else {
                    "audio/ogg".into()
                },
            ),
            None => (original, mime),
        },
    };
    let path = safe_relative(base_dir, &relative)?;
    if !path.is_file() {
        return Err("预览附件文件缺失".into());
    }
    Ok((path, content_type))
}

pub fn protocol_response(
    connection: &Connection,
    base_dir: &Path,
    id: &str,
    variant: &str,
    range: Option<&str>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let (path, content_type) = media_path(connection, base_dir, id, variant)?;
    let length = fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len();
    let parsed_range = range
        .and_then(|raw| raw.strip_prefix("bytes="))
        .and_then(|value| {
            let (start, end) = value.split_once('-')?;
            let start = start.parse::<u64>().ok()?;
            let end = if end.is_empty() {
                length.saturating_sub(1)
            } else {
                end.parse::<u64>().ok()?.min(length.saturating_sub(1))
            };
            (start <= end && start < length).then_some((start, end))
        });
    let (start, end, status) = parsed_range
        .map(|(start, end)| (start, end, 206))
        .unwrap_or((0, length.saturating_sub(1), 200));
    let count = if length == 0 { 0 } else { end - start + 1 };
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut body = Vec::with_capacity(count.min(16 * 1024 * 1024) as usize);
    file.take(count)
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    let mut builder = tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Accept-Ranges", "bytes")
        .header("Content-Length", body.len().to_string())
        .header("Access-Control-Allow-Origin", "*");
    if status == 206 {
        builder = builder.header("Content-Range", format!("bytes {start}-{end}/{length}"));
    }
    builder.body(body).map_err(|error| error.to_string())
}

pub(crate) fn file_response(
    path: &Path,
    content_type: &str,
    range: Option<&str>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let length = fs::metadata(path).map_err(|e| e.to_string())?.len();
    let parsed = range
        .and_then(|raw| raw.strip_prefix("bytes="))
        .and_then(|value| {
            let (start, end) = value.split_once('-')?;
            let start = start.parse::<u64>().ok()?;
            let end = if end.is_empty() {
                length.saturating_sub(1)
            } else {
                end.parse::<u64>().ok()?.min(length.saturating_sub(1))
            };
            (start <= end && start < length).then_some((start, end))
        });
    let (start, end, status) =
        parsed
            .map(|(s, e)| (s, e, 206))
            .unwrap_or((0, length.saturating_sub(1), 200));
    let count = if length == 0 { 0 } else { end - start + 1 };
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut body = Vec::with_capacity(count.min(16 * 1024 * 1024) as usize);
    file.take(count)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    let mut builder = tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Accept-Ranges", "bytes")
        .header("Content-Length", body.len().to_string())
        .header("Access-Control-Allow-Origin", "*");
    if status == 206 {
        builder = builder.header("Content-Range", format!("bytes {start}-{end}/{length}"));
    }
    builder.body(body).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_database;
    use std::fs;
    use tempfile::tempdir;
    #[test]
    fn accepts_only_declared_formats_and_rejects_traversal() {
        assert_eq!(media_kind(Path::new("preview.MKV")).unwrap().0, "video");
        assert!(media_kind(Path::new("payload.exe")).is_err());
        assert!(safe_relative(Path::new("C:/library"), "../secret").is_err());
    }

    #[test]
    fn imports_native_media_and_streams_byte_ranges() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("library.db")).unwrap();
        db.execute("INSERT INTO assets(id,name,description,dcc_tools_json,versions_json,formats_json,author,source_url,license,share_url,normalized_share_url,extraction_code,favorite,rating,created_at,updated_at) VALUES('asset','媒体','', '[]','[]','[]','','','','','','',0,0,'now','now')",[]).unwrap();
        let source = dir.path().join("tone.mp3");
        fs::write(&source, b"0123456789").unwrap();
        let values = import(
            &mut db,
            dir.path(),
            None,
            "asset",
            vec![source.to_string_lossy().into_owned()],
        )
        .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].processing_status, "ready");
        let response =
            protocol_response(&db, dir.path(), &values[0].id, "stream", Some("bytes=2-5")).unwrap();
        assert_eq!(response.status(), 206);
        assert_eq!(response.body(), b"2345");
        delete(&mut db, dir.path(), &values[0].id).unwrap();
        assert!(!dir
            .path()
            .join(format!("media/asset/originals/{}.mp3", values[0].id))
            .exists());
    }
}
