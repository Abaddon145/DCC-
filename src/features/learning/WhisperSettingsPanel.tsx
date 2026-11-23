import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { learningApi } from "./api";
import { loadWhisperPrefs, saveWhisperPrefs, type WhisperPrefs } from "./whisperPrefs";
import type { WhisperComponentStatus } from "./types";

export function WhisperSettingsPanel({
  libraryId,
  notify,
}: {
  libraryId: string;
  notify?: (message: string, error?: boolean) => void;
}) {
  const [status, setStatus] = useState<WhisperComponentStatus | null>(null);
  const [prefs, setPrefs] = useState<WhisperPrefs>(() => loadWhisperPrefs());
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const refresh = useCallback(async () => {
    setBusy("check");
    setError("");
    try {
      const next = await learningApi.transcriptionComponents(libraryId);
      setStatus(next);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy("");
    }
  }, [libraryId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const updatePrefs = (patch: Partial<WhisperPrefs>) => {
    const next = { ...prefs, ...patch };
    setPrefs(next);
    saveWhisperPrefs(next);
  };

  const installTools = async () => {
    if (busy) return;
    setBusy("tools");
    setError("");
    setNotice("");
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择含 tools 和 licenses 的 Whisper 资源目录",
      });
      if (typeof selected !== "string") return;
      const next = await learningApi.installTranscriptionTools(libraryId, selected);
      setStatus(next);
      setNotice("Whisper 本地组件包已安装并核验。");
      notify?.("Whisper 本地组件包已就绪");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy("");
    }
  };

  const importModel = async (modelId: "base" | "small") => {
    if (busy) return;
    setBusy(`import:${modelId}`);
    setError("");
    setNotice("");
    try {
      const selected = await open({
        multiple: false,
        title: "选择与固定清单匹配的 Whisper 模型文件",
        filters: [{ name: "Whisper GGML", extensions: ["bin"] }],
      });
      if (typeof selected !== "string") return;
      const next = await learningApi.importTranscriptionModel(libraryId, modelId, selected);
      setStatus(next);
      setNotice(`模型 ${modelId} 已导入并完成 SHA-256 校验。`);
      notify?.(`Whisper ${modelId} 模型已就绪`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy("");
    }
  };

  const downloadModel = async (modelId: "base" | "small") => {
    if (busy) return;
    setBusy(`download:${modelId}`);
    setError("");
    setNotice(`正在下载并校验 Whisper ${modelId} 模型，请稍候…`);
    try {
      const next = await learningApi.downloadTranscriptionModel(libraryId, modelId);
      setStatus(next);
      setNotice(`模型 ${modelId} 已下载并完成 SHA-256 与文件头校验。`);
      notify?.(`Whisper ${modelId} 模型已下载就绪`);
    } catch (reason) {
      setError(String(reason));
      setNotice("");
    } finally {
      setBusy("");
    }
  };

  return (
    <section className="whisper-settings-panel" aria-label="本地语音转写设置">
      <h3>学习库 · 本地语音转写（Whisper）</h3>
      <p>
        一次配置，所有学习库通用。无字幕课程可直接一键本地转写后再进行视频分析；模型与组件为本机全局数据，不进入素材库备份。
      </p>
      {error && <p className="form-warning" role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <div className="setting-row">
        <span>
          <strong>Whisper 组件状态</strong>
          <small>
            {status
              ? `Whisper ${status.version} · ${status.toolReady ? "工具已核验" : status.toolMessage}`
              : "正在检查组件状态…"}
          </small>
        </span>
        <div className="translation-settings-actions">
          <button
            type="button"
            className="secondary-button"
            disabled={!!busy}
            onClick={() => void installTools()}
          >
            {busy === "tools" ? "正在核验…" : "安装本地组件包"}
          </button>
          <button
            type="button"
            className="secondary-button"
            disabled={!!busy}
            onClick={() => void refresh()}
          >
            重新检查
          </button>
        </div>
      </div>

      <label className="setting-row">
        <span>
          默认语音模型
          <small>一键“转写并分析”默认使用的本机模型</small>
        </span>
        <select
          aria-label="默认语音模型"
          value={prefs.defaultModel}
          onChange={event => updatePrefs({ defaultModel: event.target.value === "small" ? "small" : "base" })}
        >
          <option value="base">Whisper Base（较快 · ~141 MiB）</option>
          <option value="small">Whisper Small（更准 · ~465 MiB）</option>
        </select>
      </label>

      <label className="setting-row check">
        <span>
          跳过每次本地转写的重复确认
          <small>开启后，点击“开始本地转写”或“转写并分析”不再要求每次勾选本机 CPU/磁盘确认框</small>
        </span>
        <input
          type="checkbox"
          checked={prefs.skipRepeatConsent}
          onChange={event => updatePrefs({ skipRepeatConsent: event.target.checked })}
        />
      </label>

      {status && status.models.length > 0 && (
        <div className="whisper-model-list">
          {status.models.map(item => {
            const downloading = busy === `download:${item.id}`;
            const importing = busy === `import:${item.id}`;
            return (
              <div className="setting-row" key={item.id}>
                <span>
                  <strong>
                    {item.label}（{(item.bytes / 1048576).toFixed(1)} MiB）
                  </strong>
                  <small>{item.installed ? "已安装并通过 SHA-256 校验" : item.message}</small>
                </span>
                <div className="translation-settings-actions">
                  {!item.installed && (
                    <button
                      type="button"
                      className="primary-button"
                      disabled={!!busy}
                      onClick={() => void downloadModel(item.id)}
                    >
                      {downloading ? "正在下载校验…" : "一键下载并校验"}
                    </button>
                  )}
                  <button
                    type="button"
                    className="secondary-button"
                    disabled={!!busy}
                    onClick={() => void importModel(item.id)}
                  >
                    {importing ? "正在导入…" : item.installed ? "重新校验导入" : "离线导入模型"}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
