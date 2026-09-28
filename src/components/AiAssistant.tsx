import { useEffect, useRef, useState } from "react";
import { Check, ChevronLeft, ChevronRight, Plus, Search, Sparkles, Undo2, X } from "lucide-react";
import { aiApi, aiDefaults, aiSelectionKeys, type AiEntry, type AiEntryKind, type AiKind, type AiLaunch, type AiSession } from "../lib/ai";
import { api } from "../lib/api";
import type { ContentLanguage, ProjectSummary, TranslationSettings } from "../types";
import "./AiAssistant.css";

const kinds: Record<AiKind, { name: string; detail: string; placeholder: string }> = {
  plan: { name: "创作方案", detail: "从创作方向到场景、镜头与制作任务", placeholder: "例如：制作一张黄昏沙漠遗迹静帧，使用 UE，重点是尺度感与光影。帮我拆分制作步骤和验收要点。" },
  organize: { name: "整理建议", detail: "为选中的内容建议名称、描述、标签和分类", placeholder: "例如：保留现有事实，补充便于检索的描述与标签，复用规范分类。不确定的内容标注待整理。" },
  search: { name: "自然语言检索", detail: "把创作意图转成可检查的本地搜索条件", placeholder: "例如：找适用于 UE 5 的沙漠环境和岩石，不要音效。" },
  recommend: { name: "项目选材", detail: "在有限的本地候选中推荐适合项目的内容", placeholder: "例如：为这个项目挑选环境与道具，说明使用位置、匹配依据和仍需核实的信息。" },
};
const entryNames: Record<AiEntryKind, string> = { asset: "素材", image: "图片", model: "三维模型", audio: "音频", video: "视频" };
const statusNames: Record<string, string> = { draft: "待确认发送", generating: "正在生成", ready: "待检查", applied: "已应用", undone: "已撤销", failed: "生成失败", cancelled: "已取消", interrupted: "已中断" };
interface Props { settingsOpen?: boolean; language: ContentLanguage; launch?: AiLaunch; notify: (message: string, error?: boolean) => void; onChanged: () => void; onSettings: () => void }

export function AiAssistant({ settingsOpen = false, language, launch, notify, onChanged, onSettings }: Props) {
  const [request, setRequest] = useState(() => aiDefaults(language, launch));
  const [session, setSession] = useState<AiSession | null>(null);
  const [history, setHistory] = useState<Awaited<ReturnType<typeof aiApi.list>>>({ items: [], total: 0 });
  const [offset, setOffset] = useState(0);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [settings, setSettings] = useState<TranslationSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [rename, setRename] = useState("");
  const [candidateKind, setCandidateKind] = useState<AiEntryKind>("asset");
  const [candidateQuery, setCandidateQuery] = useState("");
  const [candidates, setCandidates] = useState<{ items: AiEntry[]; total: number; offset: number }>({ items: [], total: 0, offset: 0 });
  const [searchResult, setSearchResult] = useState<{ index: number; items: AiEntry[]; total: number; offset: number } | null>(null);
  const [inspected, setInspected] = useState<AiEntry | null>(null);
  const mounted = useRef(true);
  const currentId = useRef<string | null>(null);
  currentId.current = session?.id ?? null;
  const historyVersion = useRef(0);
  const loadHistory = async () => { const version = ++historyVersion.current; const result = await aiApi.list(offset); if (mounted.current && version === historyVersion.current) setHistory(result); };
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { void loadHistory().catch(e => notify(String(e), true)); }, [offset]);
  useEffect(() => { void api.listProjects(false).then(setProjects).catch(e => notify(String(e), true)); void api.getTranslationSettings().then(setSettings).catch(e => notify(String(e), true)); }, [settingsOpen]);
  useEffect(() => {
    if (session?.status !== "generating") return;
    const id = session.id;
    const timer = window.setInterval(() => { void aiApi.get(id).then(value => { if (mounted.current && currentId.current === id) setSession(value); }).catch(() => undefined); }, 1200);
    return () => window.clearInterval(timer);
  }, [session?.id, session?.status]);
  const run = async (work: () => Promise<void>) => { setBusy(true); try { await work(); } catch (error) { if (mounted.current) notify(String(error), true); } finally { if (mounted.current) setBusy(false); } };
  const adopt = (value: AiSession) => { setSession(value); setRequest(value.request); setRename(value.name); setSelected(new Set()); setSearchResult(null); };
  const openSession = (id: string) => void run(async () => { const value = await aiApi.get(id); if (mounted.current) adopt(value); });
  const prepare = () => void run(async () => { const value = await aiApi.prepare(request); if (!mounted.current) return; adopt(value); await loadHistory(); });
  const generate = () => {
    if (!session) return;
    const id = session.id;
    setSession({ ...session, status: "generating", message: "正在请求 DeepSeek；正在生成的内容不会自动应用" });
    void run(async () => {
      try { const value = await aiApi.generate(id); if (mounted.current && currentId.current === id) adopt(value); }
      catch (e) { if (mounted.current && currentId.current === id) { const value = await aiApi.get(id); adopt(value); } throw e; }
      finally { if (mounted.current) { await loadHistory(); setSettings(await api.getTranslationSettings()); } }
    });
  };
  const apply = () => { if (!session || !selected.size || !window.confirm(`应用选中的 ${selected.size} 项建议？\n名称、描述可能被替换；标签仅追加。可撤销最近一次 AI 应用。`)) return; void run(async () => { const message = await aiApi.apply(session.id, [...selected]); notify(message); onChanged(); adopt(await aiApi.get(session.id)); await loadHistory(); }); };
  const toggle = (key: string) => setSelected(previous => { const next = new Set(previous); if (next.has(key)) next.delete(key); else next.add(key); return next; });
  const selectCandidate = (entry: AiEntry) => setRequest(previous => { const key = entry.kind === "asset" ? "assetIds" : "mediaIds"; const current = previous[key]; return { ...previous, [key]: current.includes(entry.id) ? current.filter(id => id !== entry.id) : [...current, entry.id] }; });
  const inspect = (entry: AiEntry) => setInspected(entry);
  const output = session?.output;
  const editable = !session;
  const generating = session?.status === "generating";
  const entryById = (id: string) => session?.context.entries.find(item => item.id === id);
  const results = (items: AiEntry[]) => items.map(item => <article className="ai-candidate" key={item.id}><div><span>{entryNames[item.kind]}</span><strong>{item.name}</strong><p>{item.tags.join(" · ") || "暂无标签"}</p></div><button className="secondary-button compact" onClick={() => inspect(item)}>查看元数据</button></article>);
  return <section className="ai-workbench">
    <header className="module-page-header"><div><span className="eyebrow">CREATIVE ASSISTANT</span><h1>AI 创作助手</h1><p>从想法到制作 · 建议先审阅，数据由你决定</p></div><div className="ai-actions"><button className="secondary-button" disabled={busy} onClick={() => { if (window.confirm("撤销当前素材库最近一次 AI 应用？后续修改存在冲突时将拒绝撤销。")) void run(async () => { await aiApi.undo(); notify("最近一次 AI 应用已撤销"); onChanged(); if (session) adopt(await aiApi.get(session.id)); await loadHistory(); }); }}><Undo2 size={16} />撤销最近应用</button><button className="primary-button" disabled={busy || generating} onClick={() => { setSession(null); setRequest(aiDefaults(language)); setSelected(new Set()); setSearchResult(null); }}><Plus size={16} />新草稿</button></div></header>
    <div className="ai-layout"><aside className="ai-history"><h2>本库草稿 <span>{history.total}</span></h2>{history.items.length ? history.items.map(item => <button className={session?.id === item.id ? "active" : ""} disabled={busy} key={item.id} onClick={() => openSession(item.id)}><strong>{item.name}</strong><span>{kinds[item.kind]?.name} · {statusNames[item.status] ?? item.status}</span></button>) : <p>草稿、结果与应用记录只保存在当前素材库。</p>}<div className="ai-pagination"><button aria-label="上一页草稿" disabled={!offset || busy} onClick={() => setOffset(Math.max(0, offset - 50))}><ChevronLeft size={16} /></button><span>{Math.floor(offset / 50) + 1}</span><button aria-label="下一页草稿" disabled={offset + 50 >= history.total || busy} onClick={() => setOffset(offset + 50)}><ChevronRight size={16} /></button></div></aside>
    <main className="ai-main">
      {!settings?.deepseekConfigured && <div className="ai-notice">需要你自己的 DeepSeek API Key。<button className="link-button" onClick={onSettings}>打开在线服务设置</button></div>}
      <div className="ai-mode-grid">{(Object.keys(kinds) as AiKind[]).map(kind => <button key={kind} disabled={!editable} className={request.kind === kind ? "active" : ""} onClick={() => setRequest(value => ({ ...value, kind, mode: "fast" }))}><strong>{kinds[kind].name}</strong><span>{kinds[kind].detail}</span></button>)}</div>
      {editable ? <div className="ai-compose">
        <label>创作要求<textarea aria-label="创作要求" rows={5} maxLength={6000} value={request.prompt} placeholder={kinds[request.kind].placeholder} onChange={e => setRequest(value => ({ ...value, prompt: e.target.value }))} /></label>
        <div className="ai-form-row"><label>目标项目<select value={request.projectId ?? ""} onChange={e => setRequest(value => ({ ...value, projectId: e.target.value || null }))}><option value="">{request.kind === "plan" ? "应用时创建新项目" : "不关联项目"}</option>{projects.map(project => <option key={project.id} value={project.id}>{project.name}</option>)}</select></label><label>素材内容语言<select value={request.locale} onChange={e => setRequest(value => ({ ...value, locale: e.target.value as ContentLanguage }))}><option value="zh-CN">中文</option><option value="en">English</option></select></label><label>生成模式<select disabled={request.kind !== "plan"} value={request.mode} onChange={e => setRequest(value => ({ ...value, mode: e.target.value as "fast" | "deep" }))}><option value="fast">快速生成</option><option value="deep">深入推敲（更多用量）</option></select></label></div>
        {(request.kind === "search" || request.kind === "recommend") && <label>检索范围<select value={request.scope} onChange={e => setRequest(value => ({ ...value, scope: e.target.value as typeof request.scope }))}><option value="all">素材及四类媒体</option>{Object.entries(entryNames).map(([key, name]) => <option value={key} key={key}>{name}</option>)}</select></label>}
        {request.kind === "recommend" && <label>本地召回关键词（先找出最多 50 个候选，再发送给 AI）<input value={request.recallQuery} maxLength={200} placeholder="例如：沙漠 / desert；留空仅使用明确选择的条目" onChange={e => setRequest(value => ({ ...value, recallQuery: e.target.value }))} /></label>}
        {request.kind !== "search" && <details className="ai-picker"><summary>选择上下文：{request.assetIds.length} 个素材 · {request.mediaIds.length} 个媒体（每次最多 40 个）</summary><p>仅提交文字元数据，不上传图片、模型、音视频或网盘提取码；媒体仅整理中文。</p><div className="ai-form-row"><select aria-label="候选类型" value={candidateKind} onChange={e => setCandidateKind(e.target.value as AiEntryKind)}>{Object.entries(entryNames).map(([key, name]) => <option value={key} key={key}>{name}</option>)}</select><input aria-label="本地候选关键词" value={candidateQuery} onChange={e => setCandidateQuery(e.target.value)} placeholder="输入关键词查找本地条目" /><button className="secondary-button" disabled={busy || !candidateQuery.trim()} onClick={() => void run(async () => setCandidates(await aiApi.candidates(candidateKind, candidateQuery, language)))}><Search size={15} />查找</button><button className="secondary-button" onClick={() => setRequest(value => ({ ...value, assetIds: [], mediaIds: [] }))}>清空选择</button></div>{candidates.items.map(item => <label className="ai-choice" key={item.id}><input type="checkbox" checked={[...request.assetIds, ...request.mediaIds].includes(item.id)} onChange={() => selectCandidate(item)} /><span>{item.name}<small>{entryNames[item.kind]} · {item.tags.join(" · ")}</small></span></label>)}{candidates.total > candidates.offset + candidates.items.length && <button className="link-button" disabled={busy} onClick={() => void run(async () => setCandidates(await aiApi.candidates(candidateKind, candidateQuery, language, candidates.offset + 40)))}>下一页候选</button>}</details>}
        <div className="ai-actions"><p>准备草稿不会调用在线服务。</p><button className="primary-button" disabled={busy || !request.prompt.trim()} onClick={prepare}>检查发送范围</button></div>
      </div> : session && <>
        <div className="ai-session-heading"><div><span className={`ai-status ${session.status}`}>{statusNames[session.status] ?? session.status}</span><h2>{session.name}</h2></div><details><summary>管理草稿</summary><label>草稿名称<input value={rename} maxLength={100} onChange={e => setRename(e.target.value)} /></label><div className="ai-actions"><button disabled={busy || generating || !rename.trim()} onClick={() => void run(async () => { await aiApi.rename(session.id, rename); adopt(await aiApi.get(session.id)); await loadHistory(); })}>重命名</button><button disabled={busy || generating} onClick={() => { setSession(null); setSelected(new Set()); }}>修改要求 / 重新生成</button><button className="danger" disabled={busy || generating} onClick={() => { if (window.confirm("删除此 AI 草稿及生成结果？已经应用到素材或项目的数据不会删除。")) void run(async () => { await aiApi.remove(session.id); setSession(null); await loadHistory(); }); }}>删除草稿</button></div></details></div>
        <p className="ai-prompt">{session.request.prompt}</p>
        <details className="ai-context"><summary>将发送的上下文 · {session.context.entries.length} 项 · 文字内容</summary><pre>{JSON.stringify(session.context, null, 2)}</pre></details>
        <div className="ai-notice"><strong>本次预留估算 {session.estimatedTokens.toLocaleString()} Token</strong><p>这是保守的本机额度预留，不是账单价格。翻译与创作共享月度上限 {settings?.preferences?.deepseekMonthlyLimit.toLocaleString() ?? "—"}；本月已计 {settings?.usage?.deepseekTokens.toLocaleString() ?? "0"} Token。深入推敲预留更多输出。网络重试最多一次，可能再次预留。</p><p>{session.message}</p></div>
        {session.status === "draft" && <div className="ai-actions"><span>确认将以上文字发送至 DeepSeek？</span><button className="primary-button" disabled={busy || !settings?.deepseekConfigured} onClick={generate}><Sparkles size={16} />确认发送并生成</button></div>}
        {generating && <div className="ai-actions" role="status"><span>正在生成 → 本地校验 → 等待你确认。你可以切换页面，结果会留在草稿中。</span><button className="secondary-button" onClick={() => void aiApi.cancel(session.id).catch(e => notify(String(e), true))}>取消生成</button></div>}
        {output && <section className="ai-output"><h2>{output.title}</h2><p>{output.summary}</p>{output.directions.length > 0 && <><h3>创作方向与步骤</h3><ol>{output.directions.map((v, i) => <li key={i}>{v}</li>)}</ol></>}{output.needs.length > 0 && <><h3>素材需求与待核实信息</h3><ul>{output.needs.map((v, i) => <li key={i}>{v}</li>)}</ul></>}
          {output.tasks.map((task, i) => <label className="ai-choice" key={`task:${i}`}><input type="checkbox" disabled={session.status !== "ready"} checked={selected.has(`task:${i}`)} onChange={() => toggle(`task:${i}`)} /><span><strong>{task.title}</strong><small>{[task.scene, task.shot].filter(Boolean).join(" / ") || "项目任务"}</small><p>{task.description}</p></span></label>)}
          {output.changes.map((change, i) => { const original = entryById(change.id); return <label className="ai-choice" key={`change:${i}`}><input type="checkbox" disabled={session.status !== "ready"} checked={selected.has(`change:${i}`)} onChange={() => toggle(`change:${i}`)} /><span><strong>{original?.name ?? change.id} · {{ name: "名称", description: "描述", tags: "追加标签", category: "分类 / 文件夹" }[change.field]}</strong><small>原值：{change.field === "category" ? original?.category || "未分类" : change.field === "tags" ? original?.tags.join("；") || "空" : original?.[change.field] || "空"}</small><p className="ai-proposed">{Array.isArray(change.value) ? change.value.join(change.field === "category" ? " / " : "；") : change.value}</p><small>依据：{change.reason}</small>{change.field === "category" && <small>缺少的分类仅在确认应用时创建。</small>}</span></label>; })}
          {output.recommendations.map((value, i) => <label className="ai-choice" key={`recommend:${i}`}><input type="checkbox" disabled={session.status !== "ready"} checked={selected.has(`recommend:${i}`)} onChange={() => toggle(`recommend:${i}`)} /><span><strong>{entryById(value.id)?.name ?? value.id}</strong><p>{value.reason}</p><small>仍需核实：{value.unknowns || "请自行检查许可与项目兼容性"}</small><small>已有项目关联不会被覆盖。</small></span></label>)}
          {output.searches.map((search, index) => <div className="ai-search-proposal" key={index}><span>{entryNames[search.kind]}</span><code>{search.query}</code><button className="secondary-button" disabled={busy} onClick={() => void run(async () => setSearchResult({ ...await aiApi.search(session.id, index), index }))}>运行本地搜索</button></div>)}
          {searchResult && <div className="ai-search-results"><h3>实际检索结果 · {searchResult.total} 项</h3>{results(searchResult.items)}<div className="ai-pagination"><button disabled={busy || !searchResult.offset} onClick={() => void run(async () => setSearchResult({ ...await aiApi.search(session.id, searchResult.index, Math.max(0, searchResult.offset - 40)), index: searchResult.index }))}>上一页</button><button disabled={busy || searchResult.offset + 40 >= searchResult.total} onClick={() => void run(async () => setSearchResult({ ...await aiApi.search(session.id, searchResult.index, searchResult.offset + 40), index: searchResult.index }))}>下一页</button></div></div>}
          {aiSelectionKeys(output).length > 0 && <div className="ai-apply-bar"><span>已选 {selected.size} 项 · 不勾选的内容不会写入</span><button className="secondary-button" disabled={session.status !== "ready" || busy} onClick={() => setSelected(selected.size ? new Set() : new Set(aiSelectionKeys(output)))}>{selected.size ? "清空勾选" : "勾选全部"}</button><button className="primary-button" disabled={session.status !== "ready" || busy || !selected.size} onClick={apply}><Check size={16} />确认应用所选</button></div>}
        </section>}
      </>}
    </main></div>
    {inspected && <div className="modal-backdrop" onClick={() => setInspected(null)}><section className="ai-metadata-dialog" role="dialog" aria-modal="true" aria-label="候选元数据" onClick={e => e.stopPropagation()}><button className="icon-button" aria-label="关闭候选元数据" onClick={() => setInspected(null)}><X size={18} /></button><span className="eyebrow">{entryNames[inspected.kind]}</span><h2>{inspected.name}</h2><p>{inspected.description || "暂无描述"}</p><p>{inspected.tags.join(" · ")}</p><p>软件：{inspected.software.join(" · ") || "未知"} · 版本：{inspected.versions.join(" · ") || "未知"}</p><p>格式：{inspected.formats.join(" · ") || "未知"}</p><p>许可：{inspected.license || "未知，请核实"}</p></section></div>}
  </section>;
}
