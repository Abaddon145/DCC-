import type { CommandId, GlobalPreferences, LibraryPreferences, ModuleGroupId, ModuleId } from "../types";

export const moduleRegistry: Array<{ id: ModuleId; group: ModuleGroupId; label: string; description: string }> = [
  { id: "library", group: "content", label: "素材库", description: "浏览、搜索与整理全部素材" },
  { id: "imageLibrary", group: "content", label: "图片库", description: "集中管理参考图、贴图和概念图" },
  { id: "modelLibrary", group: "content", label: "三维模型库", description: "管理并交互预览 GLB、GLTF、FBX、OBJ 和 STL" },
  { id: "audioLibrary", group: "content", label: "音频库", description: "管理音效、音乐与波形预览" },
  { id: "videoLibrary", group: "content", label: "视频库", description: "管理视频参考与代理预览" },
  { id: "projects", group: "creation", label: "创作项目", description: "项目素材、镜头任务、参考板与工程入口" },
  { id: "reference", group: "creation", label: "参考板", description: "PureRef 风格无限画布" },
  { id: "smartCollections", group: "libraryContext", label: "智能集合", description: "保存可动态更新的搜索条件" },
  { id: "favorites", group: "libraryContext", label: "收藏", description: "快速访问收藏素材" },
  { id: "recent", group: "libraryContext", label: "最近查看", description: "回到最近打开的素材" },
  { id: "tagManager", group: "manage", label: "标签管理", description: "重命名、合并和清理标签" },
  { id: "health", group: "manage", label: "素材库检查", description: "检查缺失信息和失效链接" },
  { id: "trash", group: "manage", label: "回收站", description: "恢复或永久清理已删除内容" },
];

export const moduleGroupLabels: Record<ModuleGroupId, string> = {
  content: "内容", libraryContext: "素材库快捷入口", creation: "创作", manage: "管理",
};

export const commandRegistry: Array<{ id: CommandId; label: string; keywords: string }> = [
  { id: "commandPalette", label: "打开命令面板", keywords: "command palette 命令" },
  { id: "focusSearch", label: "聚焦搜索", keywords: "search 搜索" },
  { id: "addAsset", label: "添加素材", keywords: "new asset 新建" },
  { id: "quickAdd", label: "快速录入", keywords: "clipboard 网盘" },
  { id: "settings", label: "打开设置", keywords: "preferences 个性化" },
  { id: "toggleSelection", label: "切换多选", keywords: "select batch 批量" },
  { id: "selectAll", label: "全选当前结果", keywords: "select all 全选" },
  { id: "saveAsset", label: "保存素材编辑", keywords: "save 保存" },
  { id: "deleteSelected", label: "删除选择", keywords: "delete trash 删除" },
];

export const defaultGlobalPreferences: GlobalPreferences = {
  theme: "graphite", accentColor: "#D99A42", density: "comfortable", reduceMotion: false,
  sidebarWidth: 248, detailWidth: 420,
  shortcuts: { commandPalette: "Ctrl+P", focusSearch: "Ctrl+K", addAsset: "Ctrl+N", quickAdd: "Ctrl+Shift+N", settings: "Ctrl+,", toggleSelection: "Ctrl+M", selectAll: "Ctrl+A", saveAsset: "Ctrl+S", deleteSelected: "Delete" },
};

export const defaultLibraryPreferences: LibraryPreferences = {
  moduleOrder: moduleRegistry.map(module => module.id), disabledModules: [], startupModule: "library", rememberLastContext: false,
  assetView: "grid", cardSize: "medium", coverFit: "cover",
  cardFields: { category: true, tags: true, software: true, version: true, format: true, linkStatus: true },
  defaultSort: "updated", rememberSearch: false, categoryTreeExpanded: true, lastContext: null,
  contextPaneWidths: { library: 248, imageLibrary: 220, modelLibrary: 260, audioLibrary: 220, videoLibrary: 220 },
  collapsedContextPanes: [], collapsedModuleGroups: ["manage"],
};

export function isHexColor(value: string) { return /^#[0-9A-F]{6}$/i.test(value); }

export function textColorFor(background: string) {
  if (!isHexColor(background)) return "#111111";
  const [r, g, b] = [1, 3, 5].map(index => Number.parseInt(background.slice(index, index + 2), 16));
  return (r * 299 + g * 587 + b * 114) / 1000 > 150 ? "#111111" : "#FFFFFF";
}

export function applyGlobalPreferences(preferences: GlobalPreferences) {
  const root = document.documentElement;
  root.dataset.theme = preferences.theme;
  root.dataset.density = preferences.density;
  root.dataset.reduceMotion = String(preferences.reduceMotion);
  root.style.setProperty("--accent", preferences.accentColor);
  root.style.setProperty("--accent-bright", preferences.accentColor);
  root.style.setProperty("--accent-bg", `${preferences.accentColor}1f`);
  root.style.setProperty("--accent-text", textColorFor(preferences.accentColor));
  root.style.setProperty("--sidebar-width", `${preferences.sidebarWidth}px`);
  root.style.setProperty("--detail-width", `${preferences.detailWidth}px`);
}

export function normalizeShortcut(event: KeyboardEvent) {
  const modifierOnly = ["Control", "Shift", "Alt", "Meta"].includes(event.key);
  if (modifierOnly) return "";
  const parts: string[] = [];
  if (event.ctrlKey || event.metaKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
  parts.push(key === " " ? "Space" : key);
  return parts.join("+");
}

export function shortcutMatches(event: KeyboardEvent, shortcut: string) {
  return normalizeShortcut(event).toLowerCase() === shortcut.toLowerCase();
}

export function shortcutConflicts(shortcuts: Record<string, string>) {
  const seen = new Map<string, string>();
  const conflicts = new Set<string>();
  Object.entries(shortcuts).forEach(([id, value]) => {
    const key = value.trim().toLowerCase();
    const existing = seen.get(key);
    if (key && existing) { conflicts.add(existing); conflicts.add(id); } else if (key) seen.set(key, id);
  });
  return conflicts;
}

export function enabledModules(preferences: LibraryPreferences) {
  return preferences.moduleOrder.filter(id => id === "library" || !preferences.disabledModules.includes(id));
}
