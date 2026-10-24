use super::models::{LearningSubtitle, TranscriptCue};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_BYTES: usize = 8 * 1024 * 1024;
const SELECT: &str = "SELECT id,course_id,name,format,sha256,cue_count FROM learning_subtitles";
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<LearningSubtitle> {
    Ok(LearningSubtitle {
        id: r.get(0)?,
        course_id: r.get(1)?,
        name: r.get(2)?,
        format: r.get(3)?,
        sha256: r.get(4)?,
        cue_count: r.get(5)?,
    })
}
fn time(value: &str) -> Result<u64, String> {
    let normalized = value.replace(',', ".");
    let parts = normalized.split(':').collect::<Vec<_>>();
    if !(2..=3).contains(&parts.len()) {
        return Err("字幕时间码无效".into());
    }
    let number = |s: &str| -> Result<u64, String> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err("字幕时间码无效".into());
        }
        s.parse().map_err(|_| "字幕时间码无效".into())
    };
    let (sec, ms) = parts[parts.len() - 1]
        .split_once('.')
        .ok_or("字幕缺少毫秒")?;
    if ms.len() != 3 {
        return Err("字幕毫秒必须为三位".into());
    }
    let seconds = number(sec)?;
    let minutes = number(parts[parts.len() - 2])?;
    let hours = if parts.len() == 3 {
        number(parts[0])?
    } else {
        0
    };
    if minutes >= 60 || seconds >= 60 || hours > 168 {
        return Err("字幕时间码超出范围".into());
    }
    Ok(((hours * 60 + minutes) * 60 + seconds) * 1000 + number(ms)?)
}
pub(super) fn parse(
    bytes: &[u8],
    format: &str,
    id: &str,
    duration: Option<u64>,
) -> Result<Vec<TranscriptCue>, String> {
    if bytes.len() > MAX_BYTES {
        return Err("字幕超过8MiB，未截断导入".into());
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "字幕不是有效UTF-8，请先转换编码")?
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if text.contains('\0') {
        return Err("字幕包含无效控制字符".into());
    }
    if format == "vtt"
        && !text
            .lines()
            .next()
            .is_some_and(|line| line == "WEBVTT" || line.starts_with("WEBVTT "))
    {
        return Err("缺少WEBVTT文件头".into());
    }
    let mut result: Vec<TranscriptCue> = Vec::new();
    for block in text.split("\n\n").filter(|v| !v.trim().is_empty()) {
        let lines = block.lines().collect::<Vec<_>>();
        let first = lines[0].trim();
        if format == "vtt"
            && (first.starts_with("WEBVTT")
                || first == "NOTE"
                || first.starts_with("NOTE ")
                || first == "STYLE"
                || first == "REGION")
        {
            continue;
        }
        let n = if first.contains("-->") { 0 } else { 1 };
        let (from, to) = lines
            .get(n)
            .and_then(|v| v.split_once("-->"))
            .ok_or("字幕段缺少时间码")?;
        let start = time(from.trim())?;
        let end = time(to.split_whitespace().next().ok_or("字幕结束时间无效")?)?;
        if end <= start
            || end > 604800000
            || duration.is_some_and(|d| end > d)
            || result.last().is_some_and(|cue| cue.start_ms > start)
        {
            return Err("字幕时间或顺序超出媒体范围，未截断导入".into());
        }
        let body = lines[n + 1..].join("\n");
        if body.trim().is_empty() || body.len() > 16000 || result.len() >= 200000 {
            return Err("字幕为空、文本超过16000字节或段数超过200000，未截断导入".into());
        }
        result.push(TranscriptCue {
            id: format!("{id}:{}", result.len()),
            subtitle_id: id.into(),
            start_ms: start,
            end_ms: end,
            text: body,
        });
    }
    if result.is_empty() {
        return Err("字幕没有有效时间段".into());
    }
    Ok(result)
}
pub fn import(
    db: &Connection,
    base: &Path,
    course: &str,
    source: &Path,
) -> Result<LearningSubtitle, String> {
    let c = super::store::course(db, course)?;
    let value = source.to_str().ok_or("字幕路径无效")?;
    let normal = value.strip_prefix(r"\\?\").unwrap_or(value);
    if !source.is_absolute()
        || normal.starts_with(['\\', '/'])
        || normal.get(2..).is_some_and(|s| s.contains(':'))
        || value.chars().any(char::is_control)
    {
        return Err("请选择本地普通字幕文件，拒绝网络、设备及备用数据流".into());
    }
    let format = source
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(format.as_str(), "srt" | "vtt") {
        return Err("请选择SRT或VTT字幕文件".into());
    }
    crate::media_access::reject_links(source)?;
    let source = source.canonicalize().map_err(|_| "字幕源文件不存在")?;
    crate::media_access::reject_links(&source)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let file = options.open(&source).map_err(|_| "字幕正在写入或不可读")?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("请选择普通字幕文件".into());
    }
    let mut bytes = Vec::new();
    (&file)
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let cues = parse(&bytes, &format, &id, c.duration_ms)?;
    let sha = format!("{:x}", Sha256::digest(&bytes));
    if let Some(existing) = db
        .query_row(
            &format!("{SELECT} WHERE course_id=?1 AND sha256=?2"),
            params![course, sha],
            row,
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        path(db, base, course, &existing.id)?;
        return Ok(existing);
    }
    crate::media_access::reject_links(base)?;
    let directory = base.join("learning-data/subtitles").join(course);
    crate::media_access::reject_links(&directory)?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    crate::media_access::reject_links(&directory)?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    if !directory.starts_with(base.canonicalize().map_err(|e| e.to_string())?) {
        return Err("字幕目录不在当前库内".into());
    }
    let relative = format!("learning-data/subtitles/{course}/{id}.{format}");
    let target = directory.join(format!("{id}.{format}"));
    let mut stage = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    stage.write_all(&bytes).map_err(|e| e.to_string())?;
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    stage
        .persist_noclobber(&target)
        .map_err(|e| e.to_string())?;
    let name = source
        .file_name()
        .ok_or("字幕名称无效")?
        .to_string_lossy()
        .into_owned();
    let registration = (|| {
        let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO learning_subtitles(id,course_id,name,format,relative_path,sha256,cue_count,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,course,name,format,relative,sha,cues.len()as i64,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
        {
            let mut insert=tx.prepare("INSERT INTO learning_subtitle_cues(id,subtitle_id,ordinal,start_ms,end_ms,text) VALUES(?1,?2,?3,?4,?5,?6)").map_err(|e|e.to_string())?;
            for (n, cue) in cues.iter().enumerate() {
                insert
                    .execute(params![
                        cue.id,
                        id,
                        n as i64,
                        cue.start_ms as i64,
                        cue.end_ms as i64,
                        cue.text
                    ])
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    })();
    if let Err(error) = registration {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(LearningSubtitle {
        id,
        course_id: course.into(),
        name,
        format,
        sha256: sha,
        cue_count: cues.len() as i64,
    })
}
pub fn list(db: &Connection, course: &str) -> Result<Vec<LearningSubtitle>, String> {
    super::store::course(db, course)?;
    let mut stmt = db
        .prepare(&format!(
            "{SELECT} WHERE course_id=?1 ORDER BY created_at,id"
        ))
        .map_err(|e| e.to_string())?;
    let values = stmt
        .query_map([course], row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}
fn registered(
    db: &Connection,
    base: &Path,
    course: &str,
    id: &str,
) -> Result<(PathBuf, String), String> {
    super::store::course(db, course)?;
    uuid::Uuid::parse_str(id).map_err(|_| "字幕ID无效")?;
    let (relative, sha): (String, String) = db
        .query_row(
            "SELECT relative_path,sha256 FROM learning_subtitles WHERE id=?1 AND course_id=?2",
            params![id, course],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "当前课程中没有此字幕")?;
    if !relative.starts_with(&format!("learning-data/subtitles/{course}/")) {
        return Err("字幕登记路径无效".into());
    }
    Ok((super::files::learning_file(base, &relative)?, sha))
}
pub fn path(db: &Connection, base: &Path, course: &str, id: &str) -> Result<PathBuf, String> {
    let (path, sha) = registered(db, base, course, id)?;
    if crate::file_digest::digest_file(&path)?.1 != sha {
        return Err("字幕文件已改变，请重新导入核对".into());
    }
    Ok(path)
}
pub fn page(
    db: &Connection,
    base: &Path,
    course: &str,
    id: &str,
    offset: i64,
    limit: i64,
) -> Result<crate::models::Page<TranscriptCue>, String> {
    if offset < 0 || !(1..=50).contains(&limit) {
        return Err("字幕分页范围无效，每页最多50段".into());
    }
    registered(db, base, course, id)?;
    let total = db
        .query_row(
            "SELECT cue_count FROM learning_subtitles WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut stmt=db.prepare("SELECT id,subtitle_id,start_ms,end_ms,text FROM learning_subtitle_cues WHERE subtitle_id=?1 ORDER BY ordinal LIMIT ?2 OFFSET ?3").map_err(|e|e.to_string())?;
    let items = stmt
        .query_map(params![id, limit, offset], |r| {
            Ok(TranscriptCue {
                id: r.get(0)?,
                subtitle_id: r.get(1)?,
                start_ms: r.get(2)?,
                end_ms: r.get(3)?,
                text: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(crate::models::Page {
        items,
        total,
        offset,
        limit,
    })
}

pub(crate) fn verified_cues(
    db: &Connection,
    base: &Path,
    course: &str,
    id: &str,
) -> Result<(String, Vec<TranscriptCue>), String> {
    let duration = super::store::course(db, course)?.duration_ms;
    let (path, expected) = registered(db, base, course, id)?;
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|_| "字幕资料缺失")?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("字幕资料超过8MiB".into());
    }
    let sha = format!("{:x}", Sha256::digest(&bytes));
    if sha != expected {
        return Err("字幕原件已改变，请重新导入核对".into());
    }
    let format = path
        .extension()
        .and_then(|v| v.to_str())
        .ok_or("字幕格式无效")?;
    Ok((sha, parse(&bytes, format, id, duration)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn course(db: &Connection) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let file = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','subtitle fixture',?2,'now','now')",rusqlite::params![id,file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,duration_ms,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4','fixture.mp4',120000,'now')",rusqlite::params![file,id]).unwrap();
        super::super::store::ensure_course(db, &id).unwrap().id
    }
    #[test]
    fn learning_subtitle_keeps_full_unicode_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let course = course(&db);
        let source = dir.path().join("字幕.srt");
        let text=(0..55).map(|n|format!("{}\r\n00:00:{:02},125 --> 00:00:{:02},875\r\n中文🧑🏽‍🎨第{}段\r\n完整第二行\r\n\r\n",n+1,n,n,n)).collect::<String>();
        std::fs::write(&source, format!("\u{feff}{text}")).unwrap();
        let before = std::fs::read(&source).unwrap();
        let subtitle = import(&db, dir.path(), &course, &source).expect("explicit subtitle import");
        assert_eq!(subtitle.cue_count, 55);
        assert_eq!(
            import(&db, dir.path(), &course, &source).unwrap().id,
            subtitle.id
        );
        assert_eq!(list(&db, &course).unwrap().len(), 1);
        let first = page(&db, dir.path(), &course, &subtitle.id, 0, 50).unwrap();
        assert_eq!((first.total, first.items.len()), (55, 50));
        let tail = page(&db, dir.path(), &course, &subtitle.id, 50, 50).unwrap();
        assert_eq!(tail.items[4].text, "中文🧑🏽‍🎨第54段\n完整第二行");
        assert_eq!(
            (tail.items[4].start_ms, tail.items[4].end_ms),
            (54125, 54875)
        );
        assert_eq!(
            page(&db, dir.path(), &course, &subtitle.id, 50, 50)
                .unwrap()
                .items[4]
                .id,
            tail.items[4].id
        );
        assert!(page(&db, dir.path(), &course, &subtitle.id, 0, 51).is_err());
        assert!(page(
            &db,
            dir.path(),
            &uuid::Uuid::new_v4().to_string(),
            &subtitle.id,
            0,
            50
        )
        .is_err());
        assert_eq!(std::fs::read(&source).unwrap(), before);
        let stored = path(&db, dir.path(), &course, &subtitle.id).unwrap();
        assert_eq!(std::fs::read(&stored).unwrap(), before);
        std::fs::remove_file(stored).unwrap();
        assert!(page(&db, dir.path(), &course, &subtitle.id, 0, 50).is_err());
    }
    #[test]
    fn learning_subtitle_rejects_invalid_input_without_partial_registration() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let c = course(&db);
        let source = dir.path().join("input.vtt");
        std::fs::write(
            &source,
            "WEBVTT\n\ncue-id\n00:01.100 --> 00:02.900 align:start\n完整🦀文本\n",
        )
        .unwrap();
        let valid = import(&db, dir.path(), &c, &source).unwrap();
        assert_eq!(
            page(&db, dir.path(), &c, &valid.id, 0, 50).unwrap().items[0].start_ms,
            1100
        );
        for bad in [
            "WEBVTT\n\n00:02.000 --> 00:01.000\n倒序",
            "WEBVTT\n\n00:61.000 --> 00:62.000\n坏时间",
            "WEBVTT\n\n00:01.000 --> 00:02.000\n",
            "WEBVTT\n\n00:01.000 --> 00:02.000\n坏\0内容",
            "WEBVTT\n\n00:02:00.000 --> 00:02:01.000\n超过视频",
        ] {
            std::fs::write(&source, bad).unwrap();
            assert!(import(&db, dir.path(), &c, &source).is_err());
        }
        std::fs::write(&source, vec![b'x'; 8 * 1024 * 1024 + 1]).unwrap();
        assert!(import(&db, dir.path(), &c, &source).is_err());
        assert_eq!(list(&db, &c).unwrap().len(), 1);
    }
    #[test]
    fn learning_subtitle_backup_retains_identity_and_rejects_foreign_paths() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let archive = dir.path().join("subtitle.dccassetlib");
        let source = dir.path().join("source.srt");
        fs::write(&source, "1\n00:00:01,000 --> 00:00:02,000\n完整字幕\n").unwrap();
        let (c, id) = state
            .with_library(|db, base| {
                let c = course(db);
                let sub = import(db, base, &c, &source)?;
                crate::backup::export_library(db, base, &archive)?;
                Ok((c, sub.id))
            })
            .unwrap();
        state
            .with_active_mut(|active| crate::backup::restore_library(active, &archive))
            .unwrap();
        state.with_library(|db,base|{
            assert_eq!(page(db,base,&c,&id,0,50)?.items[0].text,"完整字幕");
            assert_eq!(import(db,base,&c,&source)?.id,id);
            db.execute("UPDATE learning_subtitles SET relative_path='learning-data/subtitles/../../outside.srt' WHERE id=?1",[&id]).unwrap();
            assert!(path(db,base,&c,&id).is_err());Ok(())
        }).unwrap();
    }
}
