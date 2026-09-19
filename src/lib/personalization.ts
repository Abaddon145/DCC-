import type { CommandId, GlobalPreferences, LibraryPreferences, ModuleId } from "../types";

export const moduleRegistry: Array<{ id: ModuleId; label: string; description: string }> = [
  { id: "library", label: "素材库", description: "浏览、搜索与整理全部素材" },
  { id: "smartCollections", label: "智能集合", description: "保存可动态更新的搜索条件" },
  { id: "favorites", label: "收藏", description: "快速访问收藏素材" },
  { id: "recent", label: "最近查看", description: "回到最近打开的素材" },
  { id: "tagManager", label: "标签管理", description: "重命名、合并和清理标签" },
  { id: "health", label: "素材库检查", description: "检查缺失信息和失效链接" },
  { id: "reference", label: "参考板", description: "PureRef 风格无限画布" },
];

export const commandRegistry: Array<{ id: CommandId; label: string; keywords: string }> = [
  { id: "commandPalette", label: "打开命令面板", keywords: "command palette 命令" },
  { id: "focusSearch", label: "聚焦搜索", keywords: "search 搜索" },
  { id: "addAsset", label: "添加素材", keywords: "new asset 新建" },
  { id: "quickAdd", label: "快速录入", keywords: "clipboard 网盘" },
  { id: "settings", label: "打开设置", keywords: "preferences 个性化" },
  { id: "toggleSelection", label: "切换多选", keywords: "select batch 批量" },
];

export const defaultGlobalPreferences: GlobalPreferences = {
  theme: "graphite", accentColor: "#D99A42", density: "comfortable", reduceMotion: false,
  sidebarWidth: 248, detailWidth: 420,
  shortcuts: { commandPalette: "Ctrl+P", focusSearch: "Ctrl+K", addAsset: "Ctrl+N", quickAdd: "Ctrl+Shift+N", settings: "Ctrl+,", toggleSelection: "Ctrl+M" },
};

export const defaultLibraryPreferences: LibraryPreferences = {
  moduleOrder: moduleRegistry.map(module => module.id), disabledModules: [], startupModule: "library", rememberLastContext: false,
  assetView: "grid", cardSize: "medium", coverFit: "cover",
  cardFields: { category: true, tags: true, software: true, version: true, format: true, linkStatus: true },
  defaultSort: "updated", rememberSearch: false, categoryTreeExpanded: true, lastContext: null,
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
