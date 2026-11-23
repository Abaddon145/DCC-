import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../lib/api";
import type { LibraryTask } from "../../types";
import { learningApi } from "./api";
import { analysisTemplateLabels } from "./analysisTemplates";
import { AnalysisResult } from "./CourseAnalysisPanel";
import { loadWhisperPrefs } from "./whisperPrefs";
import type {
  CourseAnalysisDetail,
  CourseAnalysisHistoryItem,
  CourseAnalysisTemplate,
  DetectedSubtitleSource,
  LearningSubtitle,
  WhisperComponentStatus,
} from "./types";
import "./LearningWorkspace.css";

const taskLabels: Record<string, string> = {
  queued: "排队中",
  running: "进行中",
  completed: "已完成",
  failed: "失败",
  partial: "部分完成",
  cancelled: "已取消",
  interrupted: "已中断",
  stopped: "已停止",
  paused: "已暂停",
};

export interface QuickCourseAnalysisProps {
  libraryId: string;
  courseId: string;
  courseTitle?: string;
  durationMs?: number | null;
  canSeek?: boolean;
  onSeek: (ms: number) => void;
  onOpenSettings?: () => void;
  onOpenNote?: (noteId: string) => void;
}

export function QuickCourseAnalysis({
  libraryId,
  courseId,
  courseTitle,
  durationMs,
  canSeek = false,
  onSeek,
  onOpenSettings,
  onOpenNote,
}: QuickCourseAnalysisProps) {
  const [templates, setTemplates] = useState<CourseAnalysisTemplate[]>([]);
  const [template, setTemplate] = useState("general");
  const [subtitles, setSubtitles] = useState<LearningSubtitle[]>([]);
  const [detectedSubs, setDetectedSubs] = useState<DetectedSubtitleSource[]>([]);
  const [whisper, setWhisper] = useState<WhisperComponentStatus | null>(null);
  const [customFrames, setCustomFrames] = useState("");
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [task, setTask] = useState<LibraryTask | null>(null);
  const [transcribeTask, setTranscribeTask] = useState<LibraryTask | null>(null);
  const [chainAfterTranscribe, setChainAfterTranscribe] = useState(false);
  const [historyItems, setHistoryItems] = useState<CourseAnalysisHistoryItem[]>([]);
  const [selectedHistoryId, setSelectedHistoryId] = useState("");
  const [detail, setDetail] = useState<CourseAnalysisDetail | null>(null);
  const [stage, setStage] = useState("");
  const live = useRef(true);
  const running = useRef(false);

  const usableSubtitle = [...subtitles].reverse().find(row => row.cueCount > 0) ?? null;

  useEffect(() => {
    let active = true;
    const update = (event: Event) => {
      if ((event as CustomEvent<{ libraryId: string }>).detail?.libraryId !== libraryId) return;
      void learningApi
        .listCourseAnalysisTemplates(libraryId)
        .then(rows => {
          if (active) {
            setTemplates(rows);
            setTemplate(current =>
              analysisTemplateLabels[current] || rows.some(row => row.id === current) ? current : "general"
            );
          }
        })
        .catch(reason => {
          if (active) setError(`读取分析模板失败：${String(reason)}`);
        });
    };
    window.addEventListener("learning-analysis-templates-changed", update);
    return () => {
      active = false;
      window.removeEventListener("learning-analysis-templates-changed", update);
    };
  }, [libraryId]);

  const readLatest = async (id: string) => {
    let first = await learningApi.getCourseAnalysis(libraryId, courseId, id, null, 0, 50);
    let fallback = false;
    for (
      let depth = 0;
      depth < 8 && !first.result && !first.stages.items.some(item => item.status === "completed") && first.parentAnalysisId;
      depth++
    ) {
      first = await learningApi.getCourseAnalysis(libraryId, courseId, first.parentAnalysisId, null, 0, 50);
      fallback = true;
    }
    const completed = first.stages.items.find(item => item.status === "completed");
    if (!first.result && completed) {
      const selected = await learningApi.getCourseAnalysis(libraryId, courseId, first.id, completed.key, 0, 50);
      if (live.current) {
        setStage(completed.key);
        setDetail(selected);
        setSelectedHistoryId(id);
      }
    } else if (live.current) {
      setStage("");
      setDetail(first);
      setSelectedHistoryId(id);
    }
    if (fallback && live.current) setNotice("整课汇总未完成，显示先前已保存的分析片段。");
  };

  useEffect(() => {
    live.current = true;
    setReady(false);
    void Promise.all([
      learningApi.listCourseAnalysisTemplates(libraryId),
      learningApi.getCourseAnalysisDefault(libraryId),
      learningApi.listSubtitles(libraryId, courseId),
      api.listLibraryTasks(),
      learningApi.listCourseAnalyses(libraryId, courseId, 0, 10),
    ])
      .then(([rows, preferred, subs, tasks, history]) => {
        if (!live.current) return;
        setTemplates(rows);
        setTemplate(
          analysisTemplateLabels[preferred] || rows.some(row => row.id === preferred) ? preferred : "general"
        );
        setSubtitles(subs);
        setHistoryItems(history.items);
        setTask(
          tasks.find(
            item =>
              item.libraryId === libraryId &&
              item.payload.kind === "courseAnalysis" &&
              item.payload.request.courseId === courseId &&
              ["queued", "running"].includes(item.status)
          ) || null
        );
        setTranscribeTask(
          tasks.find(
            item =>
              item.libraryId === libraryId &&
              item.payload.kind === "courseTranscription" &&
              item.payload.request.courseId === courseId &&
              ["queued", "running"].includes(item.status)
          ) || null
        );
        if (history.items[0]) {
          void readLatest(history.items[0].id).catch(reason => {
            if (live.current) setError(String(reason));
          });
        }
        setReady(true);

        if (!subs.some(s => s.cueCount > 0)) {
          if (learningApi.detectSubtitles) {
            void learningApi
              .detectSubtitles(libraryId, courseId)
              .then(found => {
                if (live.current) setDetectedSubs(found);
              })
              .catch(() => undefined);
          }
          if (learningApi.transcriptionComponents) {
            void learningApi
              .transcriptionComponents(libraryId)
              .then(status => {
                if (live.current) setWhisper(status);
              })
              .catch(() => undefined);
          }
        }
      })
      .catch(reason => {
        if (live.current) setError(`读取分析设置失败：${String(reason)}`);
      });
    return () => {
      live.current = false;
    };
  }, [libraryId, courseId]);

  useEffect(() => {
    if (!task || !["queued", "running"].includes(task.status)) return;
    let active = true;
    const timer = window.setTimeout(() => {
      void api
        .getLibraryTask(task.id)
        .then(async next => {
          if (!active || !live.current) return;
          if (
            next.libraryId !== libraryId ||
            next.payload.kind !== "courseAnalysis" ||
            next.payload.request.courseId !== courseId
          ) {
            throw new Error("分析任务不属于当前课程");
          }
          if (!["queued", "running"].includes(next.status)) {
            const report = next.result;
            const id =
              report &&
              typeof report === "object" &&
              "analysisId" in report &&
              typeof report.analysisId === "string" &&
              "courseId" in report &&
              report.courseId === courseId
                ? report.analysisId
                : null;
            const history = await learningApi.listCourseAnalyses(libraryId, courseId, 0, 10);
            if (active && live.current) setHistoryItems(history.items);
            if (id && active && live.current) await readLatest(id);
            else if (active && live.current && history.items[0]) await readLatest(history.items[0].id);
          }
          if (active && live.current) setTask(next);
        })
        .catch(reason => {
          if (active && live.current) setError(`读取任务失败：${String(reason)}`);
        });
    }, 1500);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [task, libraryId, courseId]);

  const start = async (overrideSubtitleId?: string | null) => {
    if (running.current || !ready) return;
    running.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const subtitleId =
        overrideSubtitleId !== undefined ? overrideSubtitleId : usableSubtitle?.id || null;
      const frames = customFrames.trim()
        ? customFrames
            .split(/[,，\s]+/)
            .filter(Boolean)
            .map(raw => {
              const ms = Math.round(Number(raw) * 1000);
              if (!/^(?:\d+(?:\.\d{1,3})?|\.\d{1,3})$/.test(raw) || !Number.isSafeInteger(ms)) {
                throw new Error("采样时间必须是非负秒数，最多三位小数。");
              }
              return ms;
            })
        : typeof durationMs === "number" && Number.isSafeInteger(durationMs) && durationMs >= 4
          ? [...new Set([1, 2, 3].map(part => Math.floor((durationMs * part) / 4)))]
          : [];
      if (!subtitleId && !frames.length) {
        throw new Error("没有可用字幕，也无法确定视频时长。请先导入字幕或完成本地转写。");
      }
      const custom = templates.find(item => item.id === template);
      const preview = await learningApi.prepareCourseAnalysis(libraryId, courseId, {
        subtitleId,
        frameTimesMs: frames,
        ...(template === "general" ? {} : { templateId: template }),
        ...(custom ? { templateVersion: custom.version } : {}),
      });
      if (!live.current) return;
      if (preview.courseId !== courseId || preview.expiresAt * 1000 <= Date.now()) {
        throw new Error("分析准备结果无效或已过期，请重试。");
      }
      const next = await api.startLibraryTask(
        {
          kind: "courseAnalysis",
          request: { courseId, previewId: preview.previewId, stage: "segments", autoSummary: true },
        },
        libraryId
      );
      if (!live.current) return;
      if (
        next.libraryId !== libraryId ||
        next.payload.kind !== "courseAnalysis" ||
        next.payload.request.courseId !== courseId
      ) {
        throw new Error("分析任务不属于当前课程");
      }
      setTask(next);
      const cachedCount = preview.stages?.filter(s => s.cached).length ?? 0;
      const tokenNote =
        typeof preview.tokensMax === "number" && preview.tokensMax > 0
          ? `（预计上限 ${preview.tokensMax.toLocaleString("zh-CN")} Token${cachedCount > 0 ? `，复用 ${cachedCount} 段缓存` : ""}）`
          : "";
      setNotice(`分析已加入任务中心${tokenNote}。`);
    } catch (reason) {
      if (live.current) setError(String(reason));
    } finally {
      running.current = false;
      if (live.current) setBusy(false);
    }
  };

  useEffect(() => {
    if (!transcribeTask || !["queued", "running"].includes(transcribeTask.status)) return;
    let active = true;
    const timer = window.setTimeout(() => {
      void api
        .getLibraryTask(transcribeTask.id)
        .then(async next => {
          if (!active || !live.current) return;
          setTranscribeTask(next);
          if (!["queued", "running"].includes(next.status)) {
            if (next.status === "completed") {
              const subs = await learningApi.listSubtitles(libraryId, courseId);
              if (!active || !live.current) return;
              setSubtitles(subs);
              setDetectedSubs([]);
              const report = next.result;
              const newSubId =
                report &&
                typeof report === "object" &&
                "subtitleId" in report &&
                typeof report.subtitleId === "string"
                  ? report.subtitleId
                  : [...subs].reverse().find(s => s.cueCount > 0)?.id ?? null;
              if (chainAfterTranscribe) {
                setChainAfterTranscribe(false);
                setNotice("本地转写完成，正在自动启动视频分析…");
                await start(newSubId);
              } else {
                setNotice("本地转写完成，字幕已就绪。");
              }
            } else {
              setChainAfterTranscribe(false);
              setError(`本地转写未完成（${taskLabels[next.status] || next.status}）${next.message ? `：${next.message}` : ""}`);
            }
          }
        })
        .catch(reason => {
          if (active && live.current) setError(`读取转写任务失败：${String(reason)}`);
        });
    }, 1800);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [transcribeTask, chainAfterTranscribe, libraryId, courseId]);

  const importLocalSubtitle = async () => {
    if (busy || !learningApi.importSubtitle) return;
    setBusy(true);
    setError("");
    try {
      const path = await open({ multiple: false, filters: [{ name: "字幕", extensions: ["srt", "vtt"] }] });
      if (typeof path !== "string") return;
      const imported = await learningApi.importSubtitle(libraryId, courseId, path);
      if (!live.current) return;
      setSubtitles(rows => (rows.some(r => r.id === imported.id) ? rows : [...rows, imported]));
      setDetectedSubs([]);
      setNotice(`已导入字幕「${imported.name}」（${imported.cueCount} 段），可以直接开始分析。`);
    } catch (reason) {
      if (live.current) setError(String(reason));
    } finally {
      if (live.current) setBusy(false);
    }
  };

  const importDetected = async (source: DetectedSubtitleSource) => {
    if (busy || !learningApi.importDetectedSubtitle) return;
    setBusy(true);
    setError("");
    try {
      const imported = await learningApi.importDetectedSubtitle(
        libraryId,
        courseId,
        source.kind,
        source.pathOrIndex,
        source.label
      );
      if (!live.current) return;
      setSubtitles(rows => (rows.some(r => r.id === imported.id) ? rows : [...rows, imported]));
      setDetectedSubs([]);
      setNotice(`已自动载入「${imported.name}」（${imported.cueCount} 段）。`);
    } catch (reason) {
      if (live.current) setError(String(reason));
    } finally {
      if (live.current) setBusy(false);
    }
  };

  const transcribeAndAnalyze = async () => {
    if (busy || !whisper?.toolReady) return;
    const prefs = loadWhisperPrefs();
    const chosenModel =
      whisper.models.find(m => m.id === prefs.defaultModel && m.installed)?.id ||
      whisper.models.find(m => m.installed)?.id;
    if (!chosenModel) return;
    setBusy(true);
    setError("");
    setNotice("正在启动本地 Whisper 转写，完成后将自动开始视频分析…");
    try {
      const next = await api.startLibraryTask(
        { kind: "courseTranscription", request: { courseId, modelId: chosenModel } },
        libraryId
      );
      if (!live.current) return;
      setChainAfterTranscribe(true);
      setTranscribeTask(next);
    } catch (reason) {
      if (live.current) {
        setError(String(reason));
        setNotice("");
      }
    } finally {
      if (live.current) setBusy(false);
    }
  };

  const cancelRunning = async () => {
    const runningTask = activeTranscribe ? transcribeTask : activeAnalysis ? task : null;
    if (!runningTask || !api.cancelLibraryTask) return;
    setBusy(true);
    try {
      await api.cancelLibraryTask(runningTask.id);
      setChainAfterTranscribe(false);
      setNotice("已请求取消任务；已完成的阶段缓存仍会保留。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const selectStage = async (key: string) => {
    if (!detail) return;
    setStage(key);
    setError("");
    try {
      const value = await learningApi.getCourseAnalysis(libraryId, courseId, detail.id, key || null, detail.stages.offset, 50);
      if (live.current) setDetail(value);
    } catch (reason) {
      if (live.current) setError(String(reason));
    }
  };

  const changePage = async (offset: number) => {
    if (!detail) return;
    setError("");
    try {
      const value = await learningApi.getCourseAnalysis(libraryId, courseId, detail.id, null, offset, 50);
      if (live.current) {
        setStage("");
        setDetail(value);
      }
    } catch (reason) {
      if (live.current) setError(String(reason));
    }
  };

  const activeAnalysis = !!task && ["queued", "running"].includes(task.status);
  const activeTranscribe = !!transcribeTask && ["queued", "running"].includes(transcribeTask.status);
  const active = activeAnalysis || activeTranscribe;
  const whisperReady = !!whisper?.toolReady && whisper.models.some(m => m.installed);
  const canResume =
    !active &&
    !!detail &&
    !detail.staleReasons.length &&
    ["partial", "failed", "cancelled"].includes(detail.status);

  const currentStep = activeTranscribe
    ? 1
    : activeAnalysis
      ? task?.phase?.includes("汇总")
        ? 3
        : 2
      : 0;

  return (
    <section className="learning-analysis learning-quick-analysis" aria-label="课程分析">
      <h2>课程分析</h2>

      <div className={`learning-material-bar ${usableSubtitle ? "ready" : "missing"}`} role="region" aria-label="分析材料状态">
        {usableSubtitle ? (
          <>
            <span>
              <strong>材料已就绪</strong> · 字幕「{usableSubtitle.name}」（{usableSubtitle.cueCount} 段）
            </span>
            {typeof learningApi.importSubtitle === "function" && (
              <button type="button" className="secondary-button" disabled={busy || active} onClick={() => void importLocalSubtitle()}>
                更换字幕
              </button>
            )}
          </>
        ) : (
          <>
            <div>
              <strong>暂无字幕</strong>
              <small>有字幕时分析质量最佳；无字幕时将仅采样少量静帧，建议先导入或一键转写。</small>
            </div>
            <div className="learning-material-actions">
              {detectedSubs.slice(0, 2).map(src => (
                <button
                  key={`${src.kind}:${src.pathOrIndex}`}
                  type="button"
                  className="primary-button"
                  disabled={busy || active}
                  onClick={() => void importDetected(src)}
                >
                  载入：{src.label}
                </button>
              ))}
              {typeof learningApi.importSubtitle === "function" && (
                <button type="button" className="secondary-button" disabled={busy || active} onClick={() => void importLocalSubtitle()}>
                  导入 SRT / VTT
                </button>
              )}
              {whisperReady && (
                <button type="button" className="primary-button" disabled={!ready || busy || active} onClick={() => void transcribeAndAnalyze()}>
                  一键转写并分析
                </button>
              )}
              {!whisperReady && onOpenSettings && (
                <button type="button" className="secondary-button" disabled={busy || active} onClick={onOpenSettings}>
                  配置本机 Whisper
                </button>
              )}
            </div>
          </>
        )}
      </div>

      <div className="learning-template-pills" role="group" aria-label="快捷切换分析模板">
        {Object.entries(analysisTemplateLabels).map(([id, name]) => (
          <button
            key={id}
            type="button"
            className={template === id ? "primary-button" : "secondary-button"}
            disabled={!ready || busy || active}
            onClick={() => setTemplate(id)}
          >
            {name}
          </button>
        ))}
        {templates.map(item => (
          <button
            key={item.id}
            type="button"
            className={template === item.id ? "primary-button" : "secondary-button"}
            disabled={!ready || busy || active}
            onClick={() => setTemplate(item.id)}
          >
            {item.name}
          </button>
        ))}
      </div>

      <label>
        分析模板
        <select
          aria-label="分析模板"
          disabled={!ready || busy || active}
          value={template}
          onChange={event => setTemplate(event.target.value)}
        >
          {Object.entries(analysisTemplateLabels).map(([id, name]) => (
            <option key={id} value={id}>
              {name}
            </option>
          ))}
          {templates.map(item => (
            <option key={item.id} value={item.id}>
              {item.name} · 自定义
            </option>
          ))}
        </select>
      </label>

      <div className="learning-analysis-primary-row">
        <button
          type="button"
          className="primary-button"
          disabled={!ready || busy || active}
          onClick={() => void start()}
        >
          {busy ? "正在准备…" : "开始分析"}
        </button>
        {canResume && (
          <button type="button" className="secondary-button" disabled={busy} onClick={() => void start()}>
            续跑剩余阶段
          </button>
        )}
        {active && typeof api.cancelLibraryTask === "function" && (
          <button type="button" className="secondary-button" disabled={busy} onClick={() => void cancelRunning()}>
            取消分析
          </button>
        )}
      </div>

      <details className="learning-analysis-advanced">
        <summary>高级选项（自定义采样帧）</summary>
        <label>
          采样时间（秒，留空默认自动抽 3 帧）
          <input
            aria-label="自定义采样时间"
            value={customFrames}
            disabled={busy || active}
            placeholder="例如 15, 45.5, 120"
            onChange={event => setCustomFrames(event.target.value)}
          />
        </label>
        <p>使用 DeepSeek 分段分析并自动完成整课汇总，不上传完整视频；缓存命中阶段自动复用。</p>
      </details>

      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}

      {active && (
        <div className="learning-pipeline-progress" role="status">
          <ol className="learning-pipeline-steps">
            <li className={currentStep === 1 ? "active" : currentStep > 1 ? "done" : ""}>1. 准备字幕</li>
            <li className={currentStep === 2 ? "active" : currentStep > 2 ? "done" : ""}>2. 分段分析</li>
            <li className={currentStep === 3 ? "active" : ""}>3. 整课汇总</li>
          </ol>
          {activeTranscribe && transcribeTask && (
            <p>
              本地转写 · {transcribeTask.phase || taskLabels[transcribeTask.status]}
              {transcribeTask.total > 0 ? ` · ${transcribeTask.current} / ${transcribeTask.total}` : ""}
            </p>
          )}
          {activeAnalysis && task && (
            <>
              <p>
                {taskLabels[task.phase] || task.phase || taskLabels[task.status] || task.status}
                {task.total > 0 ? ` · ${task.current} / ${task.total}` : ""}
                {task.message ? ` · ${task.message}` : ""}
              </p>
              {task.total > 0 ? (
                <progress value={task.current} max={task.total} aria-label="课程分析进度" />
              ) : (
                <progress aria-label="课程分析进度" />
              )}
            </>
          )}
        </div>
      )}

      {!active && task && (
        <p role="status">
          {taskLabels[task.phase] || task.phase || taskLabels[task.status] || task.status}
          {task.total > 0 ? ` · ${task.current} / ${task.total}` : ""}
          {task.message ? ` · ${task.message}` : ""}
        </p>
      )}

      {detail && (
        <section aria-label="最近分析结果">
          <div className="learning-result-header">
            <h3>最近分析结果</h3>
            {historyItems.length > 1 && (
              <select
                aria-label="切换历史分析"
                value={selectedHistoryId || detail.id}
                onChange={event => void readLatest(event.target.value)}
              >
                {historyItems.map(item => (
                  <option key={item.id} value={item.id}>
                    {item.stage === "summary" ? "汇总" : "分段"} · {taskLabels[item.status] || item.status} ·{" "}
                    {new Date(item.createdAt * 1000).toLocaleString("zh-CN")}
                  </option>
                ))}
              </select>
            )}
          </div>
          {detail.error && <p role="alert">{detail.error}</p>}
          {detail.staleReasons.map(reason => (
            <p key={reason} role="status">
              {reason}
            </p>
          ))}
          {detail.stages.items.some(item => item.status === "completed") && (
            <details>
              <summary>查看单段分析片段</summary>
              <label>
                查看分析片段
                <select
                  aria-label="查看分析片段"
                  value={stage}
                  onChange={event => void selectStage(event.target.value)}
                >
                  <option value="">选择片段</option>
                  {detail.stages.items
                    .filter(item => item.status === "completed")
                    .map((item, index) => (
                      <option key={item.key} value={item.key}>
                        片段 {index + 1}
                      </option>
                    ))}
                </select>
              </label>
              {detail.stages.total > 50 && (
                <nav aria-label="分析片段分页">
                  <button
                    className="secondary-button"
                    disabled={detail.stages.offset === 0}
                    onClick={() => void changePage(Math.max(0, detail.stages.offset - 50))}
                  >
                    上一页
                  </button>
                  <span>
                    {detail.stages.offset + 1}—
                    {Math.min(detail.stages.offset + detail.stages.items.length, detail.stages.total)} /{" "}
                    {detail.stages.total}
                  </span>
                  <button
                    className="secondary-button"
                    disabled={detail.stages.offset + detail.stages.items.length >= detail.stages.total}
                    onClick={() => void changePage(detail.stages.offset + 50)}
                  >
                    下一页
                  </button>
                </nav>
              )}
            </details>
          )}
          {detail.result && (
            <AnalysisResult
              result={detail.result}
              canSeek={canSeek && !detail.staleReasons.length}
              onSeek={onSeek}
              libraryId={libraryId}
              courseTitle={courseTitle}
              onOpenNote={onOpenNote}
            />
          )}
        </section>
      )}
    </section>
  );
}
