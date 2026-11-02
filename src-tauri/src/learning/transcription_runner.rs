use super::{
    transcription::{self, CourseTranscriptionRequest, VerifiedTranscription},
    transcription_install::WhisperPaths,
};
use crate::{state::AppState, tasks::LibraryTask};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    time::Duration,
};

pub(crate) async fn run(
    app: tauri::AppHandle,
    task: LibraryTask,
    request: CourseTranscriptionRequest,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        state
            .with_library_if_id(&task.library_id, |db, _| {
                super::store::course(db, &request.course_id)
            })?
            .ok_or("素材库已切换")?;
        let paths = WhisperPaths::new(&state.config_directory());
        let stop = || stopped(&state, &task);
        let components = transcription::verify_components(
            &paths.tools,
            &paths.models,
            &request.model_id,
            &stop,
        )?;
        let resource = app.path().resource_dir().ok();
        let ffmpeg = crate::runtime::tools::require(resource.as_deref(), "ffmpeg")?;
        let ffprobe = crate::runtime::tools::require(resource.as_deref(), "ffprobe")?;
        let spec = transcription::component_specs()?;
        let root = ffmpeg.parent().ok_or("FFmpeg目录无效")?;
        if spec.media_hashes.len() != 9 || ffprobe.parent() != Some(root) {
            return Err("转写媒体组件清单或目录无效".into());
        }
        let mut guards = Vec::new();
        for (name, sha) in spec.media_hashes {
            guards.push(transcription::verified_file(root, &name, None, &sha, &stop)?.1);
        }
        run_local(
            &state,
            &task,
            &request,
            &components,
            &ffmpeg,
            &ffprobe,
            |exe, args, cwd, timeout, stop| {
                crate::runtime::process::capture_in(
                    exe,
                    args,
                    Some(cwd),
                    timeout,
                    stop,
                    &mut |_| {},
                )
                .map_err(|_| {
                    if stop() {
                        "本地转写已取消".into()
                    } else {
                        "本地处理进程失败或超时；请核对组件、音轨、模型和磁盘空间后重试".into()
                    }
                })
            },
        )
    })
    .await
    .map_err(|_| "本地转写执行中断，原件及已有字幕已保留".to_owned())?
}

fn task_stopped(state: &AppState, task: &LibraryTask) -> bool {
    state
        .tasks
        .get(&task.id)
        .map(|t| t.stop_requested || !matches!(t.status.as_str(), "queued" | "running"))
        .unwrap_or(true)
}
fn stopped(state: &AppState, task: &LibraryTask) -> bool {
    task_stopped(state, task) || state.active_library_id().ok().as_deref() != Some(&task.library_id)
}

pub(super) fn run_local<F>(
    state: &AppState,
    task: &LibraryTask,
    request: &CourseTranscriptionRequest,
    components: &VerifiedTranscription,
    ffmpeg: &Path,
    ffprobe: &Path,
    mut capture: F,
) -> Result<Value, String>
where
    F: FnMut(&Path, &[String], &Path, Duration, &dyn Fn() -> bool) -> Result<String, String>,
{
    let stop = || stopped(state, task);
    if stop() {
        return Err("本地转写已停止，未处理原件".into());
    }
    if !matches!(&task.payload,crate::tasks::TaskRequest::CourseTranscription(r) if r.course_id==request.course_id&&r.model_id==request.model_id)
    {
        return Err("转写任务参数不一致".into());
    }
    let base = state
        .with_library_if_id(&task.library_id, |_, base| Ok(base.to_owned()))?
        .ok_or("素材库已切换")?;
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    let course = super::store::course(&db, &request.course_id)?;
    let duration = course
        .duration_ms
        .filter(|n| *n > 0 && *n <= 604800000)
        .ok_or("缺少真实课程时长，未开始转写")?;
    let pcm_bytes = duration.checked_mul(32).ok_or("课程音频过大")?;
    if pcm_bytes > u32::MAX as u64 - 1024 * 1024 {
        return Err("完整音频超过Whisper WAV容量；请导入完整字幕，未截断音频".into());
    }
    let file_id: String = db
        .query_row(
            "SELECT primary_file_id FROM media_entries WHERE id=?1",
            [&course.media_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
    let prepared = crate::media_access::prepare_media_file(&db, &base, &file_id, &grants)?;
    let (lease, sha) = prepared.open_for_analysis(&stop)?;
    let (library_id, root_hash) = super::analysis_store::library_identity(&base)?;
    if library_id != task.library_id || stop() {
        return Err("素材库身份已改变".into());
    }
    let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!({"version":1,"library":library_id,"root":root_hash,"course":course.id,"file":file_id,"sha256":sha,"durationMs":duration,"modelId":request.model_id,"modelSha256":components.model_sha256,"whisperVersion":components.version,"components":components.component_sha256,"parameters":"pcm16-mono-16000-cpu-auto-v1"})).map_err(|_|"转写身份无法编码")?));
    let name = format!("whisper-{}-{key}.srt", request.model_id);
    let subtitles = super::subtitles::list(&db, &course.id)?;
    let cached = cache_alias(&base, &course.id, &key, &subtitles)?
        .or_else(|| subtitles.iter().find(|s| s.name == name));
    if let Some(cached) = cached {
        super::subtitles::verified_cues(&db, &base, &course.id, &cached.id)?;
        if stop() {
            return Err("本地转写已停止".into());
        }
        return Ok(json!({"courseId":course.id,"subtitleId":cached.id,"cached":true}));
    }
    let directory = base.join(".dcc/runtime/transcription");
    super::transcription_install::safe_directory(&directory)?;
    let model_bytes = std::fs::metadata(&components.model)
        .map_err(|_| "语音模型缺失")?
        .len();
    crate::runtime::tools::ensure_space(
        &directory,
        pcm_bytes
            .saturating_add(model_bytes)
            .saturating_add(32 * 1024 * 1024),
    )?;
    let work = tempfile::Builder::new()
        .prefix("transcribe-")
        .tempdir_in(&directory)
        .map_err(|e| e.to_string())?;
    let update = |current, phase: &str| {
        state.tasks.update(
            &task.id,
            current,
            4,
            course.title.clone(),
            phase.into(),
            None,
        );
    };
    update(0, "提取完整音轨");
    let args = [
        "-nostdin",
        "-v",
        "error",
        "-n",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        "mov,matroska,webm,avi,asf",
        "-i",
    ]
    .map(str::to_owned)
    .into_iter()
    .chain(Some(lease.to_string_lossy().into_owned()))
    .chain(
        [
            "-map",
            "0:a:0",
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            "-rf64",
            "never",
            "-f",
            "wav",
            "audio.wav",
        ]
        .map(str::to_owned),
    )
    .collect::<Vec<_>>();
    capture(
        ffmpeg,
        &args,
        work.path(),
        Duration::from_secs((duration / 1000 * 10 + 120).min(86400)),
        &stop,
    )?;
    if stop() {
        return Err("本地转写已停止".into());
    }
    let wav = work.path().join("audio.wav");
    crate::media_access::reject_links(&wav)?;
    let size = std::fs::metadata(&wav).map_err(|_| "音轨提取未生成文件")?;
    if !size.is_file() || size.len() < 44 || size.len() > pcm_bytes.saturating_add(1024 * 1024) {
        return Err("完整音轨大小无效，未继续转写".into());
    }
    update(1, "核验音轨");
    let args = [
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        "wav",
        "audio.wav",
    ]
    .map(str::to_owned);
    let probe = capture(ffprobe, &args, work.path(), Duration::from_secs(30), &stop)?;
    let raw: Value = serde_json::from_str(&probe).map_err(|_| "音轨信息无效")?;
    let info = crate::media_engine::probe::parse(&raw)?;
    if info.codec != "pcm_s16le"
        || info.format != "wav"
        || !info
            .duration
            .is_some_and(|s| s > 0.0 && s * 1000.0 <= duration as f64 + 1000.0)
        || !raw["streams"].as_array().is_some_and(|rows| {
            rows.iter().any(|s| {
                s["codec_type"] == "audio" && s["sample_rate"] == "16000" && s["channels"] == 1
            })
        })
    {
        return Err("音轨不符合PCM16/16kHz/单声道或真实时长，未继续转写".into());
    }
    if stop() {
        return Err("本地转写已停止".into());
    }
    let linked = work.path().join("model.bin");
    if std::fs::hard_link(&components.model, &linked).is_err() {
        let mut source = std::fs::File::open(&components.model).map_err(|_| "模型无法读取")?;
        let mut target = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&linked)
            .map_err(|_| "无法准备临时模型")?;
        std::io::copy(&mut source, &mut target).map_err(|_| "临时模型复制失败，请检查磁盘空间")?;
    }
    let threads = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 8))
        .unwrap_or(1)
        .to_string();
    let args = [
        "-m",
        "model.bin",
        "-f",
        "audio.wav",
        "-l",
        "auto",
        "-t",
        &threads,
        "-ng",
        "-oj",
        "-of",
        "transcript",
        "-np",
    ]
    .map(str::to_owned);
    update(2, "本地识别，耗时取决于音轨与CPU");
    capture(
        &components.executable,
        &args,
        work.path(),
        Duration::from_secs((duration / 1000 * 30 + 120).min(86400)),
        &stop,
    )?;
    if stop() {
        return Err("本地转写已停止".into());
    }
    let output = work.path().join("transcript.json");
    crate::media_access::reject_links(&output)?;
    let mut bytes = Vec::new();
    std::fs::File::open(&output)
        .map_err(|_| "Whisper未生成完整结果")?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "转写结果不可读")?;
    let parsed =
        transcription::parse_whisper_json(&bytes, &uuid::Uuid::new_v4().to_string(), duration)?;
    let srt = transcription::transcript_srt(&parsed)?;
    let path = work.path().join(&name);
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|_| "转写字幕暂存失败")?;
    file.write_all(&srt)
        .and_then(|_| file.sync_all())
        .map_err(|_| "转写字幕保存失败")?;
    drop(file);
    update(3, "登记已校验字幕");
    if stop() {
        return Err("本地转写已停止".into());
    }
    // This lease is retained through publication. The same registered task is
    // still active, so TaskRegistry excludes restore/switch before this short gate.
    let saved = super::subtitles::import_guarded(&db, &base, &course.id, &path, |tx| {
        if task_stopped(state, task) {
            return Err("转写已停止，未登记字幕".into());
        }
        state
            .with_library_if_id(&task.library_id, |_, current| {
                let now = super::store::course(&tx, &course.id)?;
                let current_file: String = tx
                    .query_row(
                        "SELECT primary_file_id FROM media_entries WHERE id=?1",
                        [&now.media_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if current != base
                    || now.duration_ms != course.duration_ms
                    || current_file != file_id
                    || !prepared.matches_current(&tx, &base, &file_id, &grants)
                    || task_stopped(state, task)
                {
                    return Err("课程原件、时长、素材库或授权已变化，未登记字幕".into());
                }
                tx.commit().map_err(|e| e.to_string())
            })?
            .ok_or_else(|| "素材库已切换，未登记字幕".into())
    })?;
    update(4, "本地转写完成");
    let cache = state.with_library_if_id(&task.library_id, |_, current| {
        if current != base || task_stopped(state, task) {
            return Err("字幕已保存，但缓存登记已停止".into());
        }
        save_alias(&base, &course.id, &key, &saved)
    });
    let cache_warning = match cache {
        Ok(Some(())) => None,
        Ok(None) => Some("字幕已保存，库已切换，未登记转写缓存".to_owned()),
        Err(error) => Some(format!("字幕已保存，缓存登记失败：{error}")),
    };
    Ok(
        json!({"courseId":course.id,"subtitleId":saved.id,"cached":false,"language":parsed.language,"warning":cache_warning}),
    )
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TranscriptAlias {
    version: u32,
    fingerprint: String,
    subtitle_id: String,
    sha256: String,
}
fn alias_relative(course: &str, key: &str) -> String {
    format!("learning-data/transcriptions/{course}/{key}.json")
}
fn cache_alias<'a>(
    base: &Path,
    course: &str,
    key: &str,
    subtitles: &'a [super::models::LearningSubtitle],
) -> Result<Option<&'a super::models::LearningSubtitle>, String> {
    let relative = alias_relative(course, key);
    crate::media_access::reject_links(&base.join(&relative))?;
    if !base.join(&relative).exists() {
        return Ok(None);
    }
    let path = super::files::learning_file(base, &relative)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(2049)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let alias = serde_json::from_slice::<TranscriptAlias>(&bytes)
        .ok()
        .filter(|a| bytes.len() <= 2048 && a.version == 1 && a.fingerprint == key);
    Ok(alias.and_then(|a| {
        subtitles
            .iter()
            .find(|s| s.id == a.subtitle_id && s.sha256 == a.sha256)
    }))
}
fn save_alias(
    base: &Path,
    course: &str,
    key: &str,
    subtitle: &super::models::LearningSubtitle,
) -> Result<(), String> {
    let target = base.join(alias_relative(course, key));
    crate::media_access::reject_links(&target)?;
    let parent = target.parent().ok_or("转写缓存目录无效")?;
    super::transcription_install::safe_directory(parent)?;
    let mut stage = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let alias = TranscriptAlias {
        version: 1,
        fingerprint: key.into(),
        subtitle_id: subtitle.id.clone(),
        sha256: subtitle.sha256.clone(),
    };
    stage
        .write_all(&serde_json::to_vec(&alias).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    // Replace only this backend-generated cache alias, never subtitle/original bytes.
    crate::media_access::reject_links(&target)?;
    stage.persist(&target).map_err(|e| e.to_string())?;
    Ok(())
}
