use super::{analysis, analysis_models::*, analysis_store as store};
use crate::{state::AppState, tasks::LibraryTask, translation_service::CreativeReply};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(crate) async fn run(
    app: tauri::AppHandle,
    task: LibraryTask,
    request: CourseAnalysisRequest,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        tauri::async_runtime::block_on(run_segments(
            &state,
            &task,
            &request,
            |body, images, cancel| {
                let service = &state.translation_service;
                let limit = state.translation_preferences();
                async move {
                    service
                        .course_json_with_usage(
                            &body,
                            limit?.deepseek_monthly_limit,
                            &cancel,
                            images,
                        )
                        .await
                }
            },
        ))
    })
    .await
    .map_err(|_| "课程分析执行中断，已完成阶段仍保留；请重新预览".to_string())?
}

pub(crate) async fn run_segments<F, Fut>(
    state: &AppState,
    task: &LibraryTask,
    request: &CourseAnalysisRequest,
    mut exchange: F,
) -> Result<Value, String>
where
    F: FnMut(Value, usize, Arc<AtomicBool>) -> Fut,
    Fut: std::future::Future<Output = Result<CreativeReply, String>>,
{
    if request.stage != CourseAnalysisPhase::Segments {
        return Err("汇总需要单独预览批准，不能使用分段批准".into());
    }
    let (_lease, cancel) = crate::ai::acquire_request(&task.id)?;
    let stopped = || {
        cancel.load(Ordering::SeqCst)
            || state
                .tasks
                .get(&task.id)
                .map(|t| t.stop_requested || !matches!(t.status.as_str(), "queued" | "running"))
                .unwrap_or(true)
    };
    let stop = || stopped() || state.active_library_id().ok().as_deref() != Some(&task.library_id);
    let (input, live) = analysis::load_confirmed_controlled(
        state,
        &task.library_id,
        &request.course_id,
        &request.preview_id,
        &stop,
    )?;
    let limit = state.translation_preferences()?.deepseek_monthly_limit;
    if input.preview.tokens_max > 0
        && state
            .translation_service
            .usage()?
            .deepseek_tokens
            .saturating_add(input.preview.tokens_max)
            > limit
    {
        return Err("课程预留超过共享月度上限，未使用此次批准".into());
    }
    // TaskRegistry already excludes library switching/maintenance for this active
    // task. Copying many cached results must not hold the app's library mutex.
    let base = state
        .with_library_if_id(&task.library_id, |_, base| Ok(base.to_owned()))?
        .ok_or("素材库已切换，未开始分析")?;
    let mut worker = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    if stop() {
        return Err("课程分析已停止，未开始分析".into());
    }
    let id = store::begin_analysis(&mut worker, &input, chrono::Utc::now().timestamp())?;
    drop(worker);
    drop(live);
    let total = input.preview.stages.len();
    let mut completed = input.preview.stages.iter().filter(|s| s.cached).count();
    let report = |completed: usize| json!({"analysisId":id,"courseId":input.preview.course_id,"completedStages":completed,"totalStages":total,"stage":"segments"});
    state.tasks.update(
        &task.id,
        completed,
        total,
        String::new(),
        "分段分析".into(),
        Some(report(completed)),
    );
    let outcome = async {
        for (stage, segment) in input.preview.stages.iter().zip(&input.segments) {
            if stop() {
                return Err("课程分析已停止；已发送请求可能产生用量".into());
            }
            if stage.cached {
                continue;
            }
            let live = analysis::check_sources_controlled(state, &task.library_id, &input, &stop)?;
            let body = analysis::request_body(&input, segment)?;
            if crate::translation_service::course_budget(&body, segment.frames.len())?
                != stage.tokens_max
            {
                return Err("课程请求预算与批准不一致，请重新预览".into());
            }
            drop(live);
            let watcher = async {
                loop {
                    if stop() {
                        cancel.store(true, Ordering::SeqCst);
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                }
            };
            let reply = tokio::select! {
                reply=exchange(body,segment.frames.len(),cancel.clone()) => reply,
                _=watcher => Err("课程分析已停止；已发送请求的预留仍保留".into()),
            };
            let (answer, used) = match reply {
                Ok(reply) => (
                    reply.content.and_then(|raw| {
                        super::analysis_result::parse_course_result(
                            &raw,
                            &CourseScope {
                                start_ms: segment.start_ms,
                                end_ms: segment.end_ms,
                                cues: &segment.cues,
                                frames: &segment.frames,
                            },
                        )
                    }),
                    reply.used,
                ),
                Err(error) => (Err(error), None),
            };
            // Hold the newly checked original read lease through the transaction.
            let live = analysis::check_sources_controlled(state, &task.library_id, &input, &stop)?;
            state
                .with_library_mut_if_id(&task.library_id, |db, _| {
                    if stopped() {
                        return Err("课程分析已停止，响应未写入".into());
                    }
                    store::record_stage(
                        db,
                        &input,
                        &id,
                        &stage.key,
                        answer.as_ref().map_err(String::as_str),
                        used,
                    )
                })?
                .ok_or("素材库已切换，响应未写入")?;
            drop(live);
            answer?;
            completed += 1;
            state.tasks.update(
                &task.id,
                completed,
                total,
                String::new(),
                "分段分析".into(),
                None,
            );
        }
        Ok::<_, String>(())
    }
    .await;
    let status = if stop() {
        "cancelled"
    } else if outcome.is_ok() {
        "completed"
    } else if completed > 0 {
        "partial"
    } else {
        "failed"
    };
    let error = outcome.as_ref().err().map(String::as_str).unwrap_or("");
    state
        .with_library_if_id(&task.library_id, |db, _| {
            store::finish_analysis(db, &input, &id, status, error)
        })?
        .ok_or("素材库已切换，未写入旧库任务状态")?;
    state.tasks.update(
        &task.id,
        completed,
        total,
        String::new(),
        status.into(),
        Some(report(completed)),
    );
    outcome?;
    Ok(report(completed))
}
