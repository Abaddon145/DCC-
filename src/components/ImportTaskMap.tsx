import { useState } from "react";

export const importTasks = [
  { id: "table", entry: "批量导入", place: "普通素材", summary: "Excel / CSV 素材表。先预览，确认后才写入。" },
  { id: "fabLibrary", entry: "同步已购 Fab", place: "普通素材", summary: "在栈藏内登录 Epic 后读取已购库。先对照现有素材去重，确认后才写入商品记录，不下载 UE 原件。" },
  { id: "quick", entry: "快速录入", place: "普通素材", summary: "解析百度网盘分享文本，再进入素材表单确认。" },
  { id: "scan", entry: "扫描 / 导入文件", place: "全部资产", summary: "扫描只生成清单，不会自动入库；确认后才复制原件，不移动源文件。" },
  { id: "media", entry: "导入文件", place: "图片 / 模型 / 音频 / 视频库", summary: "把当前媒体库支持的原件复制进来。模型不走全部资产扫描。" },
  { id: "learningVideo", entry: "导入教程", place: "学习库", summary: "本地教程视频作为课程加入学习库，原文件不会被删除。" },
  { id: "learningMigrate", entry: "整理与迁入", place: "学习库", summary: "迁入你明确选择的 Markdown 或课程资料，不写入素材表。" },
  { id: "notes", entry: "导入", place: "知识库", summary: "导入 Markdown 或笔记包，只进入知识库。" },
] as const;

export type ImportTaskId = (typeof importTasks)[number]["id"];

export function importTask(id: ImportTaskId) {
  const task = importTasks.find(item => item.id === id);
  if (!task) throw new Error(`未知导入任务：${id}`);
  return task;
}

export function ImportTaskMap({ current, initiallyOpen = false }: { current: ImportTaskId; initiallyOpen?: boolean }) {
  const [open, setOpen] = useState(initiallyOpen);
  const task = importTask(current);
  return <section className="import-task-map" aria-label="导入任务对照">
    <div className="import-task-current">
      <span>当前导入任务</span>
      <strong>{task.entry}</strong>
      <small>{task.place} · {task.summary}</small>
    </div>
    <button type="button" className="secondary-button compact" aria-expanded={open} onClick={() => setOpen(value => !value)}>{open ? "收起导入任务对照" : "查看全部导入任务"}</button>
    {open && <>
      <p className="import-task-note">各入口仍分开执行，对照只说明该用哪一个。</p>
      <ol aria-label="全部导入任务">{importTasks.map(item => <li key={item.id} aria-current={item.id === current ? "true" : undefined}><strong>{item.entry}</strong><span>{item.place}</span><small>{item.summary}</small></li>)}</ol>
    </>}
  </section>;
}
