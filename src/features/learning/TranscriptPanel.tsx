import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { LoadFeedback } from "../../components/LoadFeedback";
import { useRequestGate } from "../../lib/workspace";
import { learningApi } from "./api";
import { learningTime } from "./LearningControls";
import { LocalTranscriptionPanel } from "./LocalTranscriptionPanel";
import type { LearningSubtitle, TranscriptCue } from "./types";

interface Props {
  libraryId: string;
  courseId: string;
  sessionId?: string;
  positionMs?: number;
  onSeek: (ms: number) => void;
  onSelection?: (cues: TranscriptCue[]) => void;
}

export function TranscriptPanel(props: Props) {
  return <TranscriptContent key={`${props.libraryId}:${props.courseId}`} {...props} />;
}

function TranscriptContent({ libraryId, courseId, sessionId, positionMs, onSeek, onSelection }: Props) {
  const [local, setLocal] = useState(false);
  const [subtitles, setSubtitles] = useState<LearningSubtitle[]>([]);
  const [id, setId] = useState("");
  const [items, setItems] = useState<TranscriptCue[]>([]);
  const [offset, setOffset] = useState(0);
  const [total, setTotal] = useState(0);
  const [selected, setSelected] = useState<TranscriptCue[]>([]);
  const [query, setQuery] = useState("");
  const [autoFollow, setAutoFollow] = useState(true);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const gate = useRequestGate();
  const selectionCallback = useRef(onSelection);
  selectionCallback.current = onSelection;
  const activeCueEl = useRef<HTMLLIElement | null>(null);
  const lastFollowedCueId = useRef("");

  useEffect(() => {
    selectionCallback.current?.([]);
  }, []);

  useEffect(() => {
    const ticket = gate.begin();
    setLoading(true);
    setError("");
    void learningApi
      .listSubtitles(libraryId, courseId)
      .then(rows => {
        if (gate.isCurrent(ticket)) {
          setSubtitles(rows);
          setId(value => (rows.some(row => row.id === value) ? value : rows[0]?.id || ""));
          setLoading(false);
        }
      })
      .catch(reason => {
        if (gate.isCurrent(ticket)) {
          setError(String(reason));
          setLoading(false);
        }
      });
  }, [libraryId, courseId, retry, gate]);

  useEffect(() => {
    let active = true;
    setItems([]);
    setTotal(0);
    if (!id) return;
    setLoading(true);
    setError("");
    const trimmed = query.trim();
    const request = trimmed
      ? learningApi.searchTranscriptCues(libraryId, courseId, id, trimmed, offset, 50)
      : learningApi.transcriptPage(libraryId, courseId, id, offset, 50);
    void request
      .then(page => {
        if (active) {
          setItems(page.items);
          setTotal(page.total);
          setLoading(false);
        }
      })
      .catch(reason => {
        if (active) {
          setError(String(reason));
          setLoading(false);
        }
      });
    return () => {
      active = false;
    };
  }, [libraryId, courseId, id, query, offset, retry]);

  const activeCueId =
    positionMs != null
      ? items.find(cue => positionMs >= cue.startMs && positionMs < cue.endMs)?.id ?? ""
      : "";

  useEffect(() => {
    if (!autoFollow || !activeCueId || activeCueId === lastFollowedCueId.current) return;
    lastFollowedCueId.current = activeCueId;
    activeCueEl.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
  }, [autoFollow, activeCueId]);

  const importFile = async () => {
    if (busy) return;
    setBusy(true);
    setError("");
    const ticket = gate.begin();
    try {
      const path = await open({ multiple: false, filters: [{ name: "字幕", extensions: ["srt", "vtt"] }] });
      if (typeof path !== "string" || !gate.isCurrent(ticket)) return;
      const subtitle = await learningApi.importSubtitle(libraryId, courseId, path);
      if (gate.isCurrent(ticket)) {
        setSubtitles(rows => (rows.some(row => row.id === subtitle.id) ? rows : [...rows, subtitle]));
        setId(subtitle.id);
        setQuery("");
        setOffset(0);
        setSelected([]);
        selectionCallback.current?.([]);
        setRetry(value => value + 1);
      }
    } catch (reason) {
      if (gate.isCurrent(ticket)) setError(String(reason));
    } finally {
      if (gate.isCurrent(ticket)) setBusy(false);
    }
  };

  const load = async () => {
    if (!sessionId || !id || busy) return;
    setBusy(true);
    setError("");
    try {
      await learningApi.loadSubtitle(libraryId, sessionId, id);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (cue: TranscriptCue) => {
    const next = selected.some(row => row.id === cue.id)
      ? selected.filter(row => row.id !== cue.id)
      : [...selected, cue];
    setSelected(next);
    selectionCallback.current?.(next);
  };

  const clearSelection = () => {
    setSelected([]);
    selectionCallback.current?.([]);
  };

  return (
    <section className="learning-transcript" aria-label="课程字幕">
      <header>
        <h2>课程字幕</h2>
        <div className="learning-panel-actions">
          <button className="secondary-button" aria-expanded={local} onClick={() => setLocal(value => !value)}>
            {local ? "收起本地转写" : "本地转写"}
          </button>
          <button className="secondary-button" disabled={busy || loading} onClick={() => void importFile()}>
            {busy ? "正在处理…" : "导入 SRT / VTT"}
          </button>
        </div>
      </header>
      {local && (
        <LocalTranscriptionPanel
          libraryId={libraryId}
          courseId={courseId}
          onComplete={next => {
            setId(next);
            setQuery("");
            setOffset(0);
            setSelected([]);
            selectionCallback.current?.([]);
            setRetry(value => value + 1);
          }}
        />
      )}
      {!!subtitles.length && (
        <div className="learning-panel-actions">
          <label>
            字幕文件
            <select
              value={id}
              disabled={busy}
              onChange={event => {
                setId(event.target.value);
                setQuery("");
                setOffset(0);
                setSelected([]);
                selectionCallback.current?.([]);
              }}
            >
              {subtitles.map(subtitle => (
                <option key={subtitle.id} value={subtitle.id}>
                  {subtitle.name} · {subtitle.cueCount} 段
                </option>
              ))}
            </select>
          </label>
          <button className="secondary-button" disabled={!sessionId || !id || busy || loading || !!error} onClick={() => void load()}>
            在播放器加载
          </button>
          <button
            type="button"
            className={autoFollow ? "secondary-button active" : "secondary-button"}
            aria-pressed={autoFollow}
            onClick={() => setAutoFollow(v => !v)}
          >
            {autoFollow ? "自动跟随：开" : "自动跟随：关"}
          </button>
        </div>
      )}
      {!!subtitles.length && (
        <div className="learning-transcript-search">
          <input
            type="search"
            aria-label="搜索字幕内容"
            placeholder="搜索字幕关键词…"
            maxLength={200}
            value={query}
            onChange={event => {
              setQuery(event.target.value);
              setOffset(0);
            }}
          />
          {selected.length > 0 && (
            <button type="button" className="secondary-button" onClick={clearSelection}>
              清空已选 ({selected.length})
            </button>
          )}
        </div>
      )}
      {error && (
        <div role="alert">
          {error}
          <button className="secondary-button" onClick={() => setRetry(value => value + 1)}>
            重新读取字幕
          </button>
        </div>
      )}
      {loading ? (
        <LoadFeedback label="正在读取字幕…" />
      ) : !subtitles.length ? (
        <p>尚未导入字幕</p>
      ) : (
        <>
          <p role="status">
            {query.trim() ? `命中 ${total} 段` : `共 ${total} 段`} · 已选 {selected.length} 段；跨页选择保留完整原文。
          </p>
          <nav aria-label="字幕分页">
            <button
              className="secondary-button"
              aria-label="上一页字幕"
              disabled={!offset || loading}
              onClick={() => setOffset(value => Math.max(0, value - 50))}
            >
              上一页
            </button>
            <span>
              {Math.floor(offset / 50) + 1} / {Math.max(1, Math.ceil(total / 50))}
            </span>
            <button
              className="secondary-button"
              aria-label="下一页字幕"
              disabled={offset + items.length >= total || loading}
              onClick={() => setOffset(value => value + 50)}
            >
              下一页
            </button>
          </nav>
          {items.length === 0 ? (
            <p>没有匹配的字幕段落。</p>
          ) : (
            <ol className="learning-cues" start={offset + 1}>
              {items.map((cue, n) => {
                const isActive = cue.id === activeCueId;
                return (
                  <li
                    key={cue.id}
                    ref={isActive ? activeCueEl : undefined}
                    className={isActive ? "active-cue" : undefined}
                    aria-current={isActive ? "time" : undefined}
                  >
                    <input
                      type="checkbox"
                      aria-label={`选择第 ${offset + n + 1} 段字幕`}
                      checked={selected.some(row => row.id === cue.id)}
                      onChange={() => toggle(cue)}
                    />
                    <div>
                      <button
                        className="secondary-button"
                        aria-label={`跳转到 ${learningTime(cue.startMs)} · 第 ${offset + n + 1} 段`}
                        onClick={() => onSeek(cue.startMs)}
                      >
                        {learningTime(cue.startMs)}—{learningTime(cue.endMs)}
                      </button>
                      <p>{cue.text}</p>
                    </div>
                  </li>
                );
              })}
            </ol>
          )}
        </>
      )}
    </section>
  );
}
