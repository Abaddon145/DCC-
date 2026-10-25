# Task10：统一工作区三态面板

> **For agentic workers:** 使用 executing-plans，按已批准主计划顺序实施；不增加产品范围。

**Goal:** 学习区与知识库共享收起/临时/固定面板行为，保留原生播放会话及笔记草稿。
**Architecture:** 前端纯状态函数 + 一个可复用面板组件，复用现有 workspace context、外观宽度、保存屏障和原生几何观察。不开新数据库表，不创建第二套偏好服务。
**Tech Stack:** 已有 React/TypeScript、CSS、Vitest、Tauri；无新依赖。
**Spec:** `C:/Users/14582/Documents/ChatGPT/视频下载软件/docs/superpowers/specs/2026-10-21-stackdcc-yingliu-merge-design.md` 4.3及主计划Task10。

## 约束

- 基线d568d04；只改栈藏，不自动合并/发布/安装，不触及真实源库。
- 收起后入口可见；同侧只有一个当前tab，已访问内容保留DOM及滚动/搜索状态。
- 临时面板外点/Esc收起，对话框、IME和输入优先；固定宽度可拖拽并双击复位。
- 布局只更新原生视口，不重建mpv；peek遮挡以已有data-native-overlay/几何观察守卫处理。
- 持久化只写closed/pinned、tab、固定宽度；临时响应式宽度不写回偏好，专注不覆盖退出布局。
- 真实DPI/跨屏留Task14，不用状态模拟冒充验收。保留v0.23和所有已完成能力。

## 10a 状态边界

**Files:** 新 `src/lib/workspacePanels.ts` / `.test.ts`；本子阶段不接UI，不重复建立全局偏好。

**Interfaces:**
- `PanelState {mode:'closed'|'peek'|'pinned';width:number;tab:string}`。
- `PanelLimits {min:number;max:number;defaultWidth:number}` 由真实调用方传已有宽度范围，不猜一套所有场景通用宽度。
- `normalizePanel(raw,fallback,limits,tabs)->PanelState`：旧context缺panel时保持既有外观宽度；只接收合法mode/有限数字/已知tab，未知退回fallback，宽度取整并夹在界限。
- `reducePanel(state,event,limits)`，event为toggle{tab}/pin/close/dismiss/resize{width}/resetWidth。切换固定面板tab保留固定；同tab再次点击收起；dismiss仅收起peek；resize仅改变pinned且数字有限。
- `PanelLayout {left,right,focusSnapshot:{left,right}|null}`；`togglePanelFocus(layout)` 原样保存/恢复两侧状态；`setLayoutPanel(layout,side,panel)` 若在专注中则恢复另一侧并退出专注，再应用明确操作。
- `persistPanels(layout)->{left,right}` 使用focusSnapshot（如有），peek只序列化为closed，保留tab与固定宽度，不序列化临时专注。

- [x] RED：旧/损坏偏好、同侧切换、dismiss不动固定、NaN/无限/越界拖宽、双击复位、专注退出完整恢复与专注中明确操作、持久化不保存peek或临时收起。
- [x] GREEN：最小纯函数，无DOM/网络/文件副作用；`npm test -- src/lib/workspacePanels.test.ts` 六项通过。

## 后续顺序

1. 10b读取实际挂载点后固定 `WorkspacePanel` Props：窄入口栏、lazy首次访问/之后保活、peek外点/Esc/IME/modal优先、固定拖宽与键盘替代、响应式CSS、原生遮挡标记。先行为测试再实现。
2. 10c学习区接入并测试切面板不重建播放/不写原件；随后知识库接入现有导航、列表与按需信息区，不重建编辑器。导航查询复用当前数据，缺少的收件箱/项目过滤先明确真实数据接口，不做只改标签的假视图。
3. 复用workspace context持久化，规范旧偏好；完整回归、构建、证据与本地提交。真实IME/长文草稿/原生窗口/DPI在Task14集中验收。

## Review Focus

- 以peek为原因的native遮挡结束后必须恢复同一播放会话。
- 面板tab变化不能重建已有编辑器或丢搜索/滚动/草稿。
- 临时小窗口不改变保存的固定宽度。
- 关闭对话框/中文候选或输入框时不能先抢Escape。
- 专注期间保存/重启不能把原固定布局永久变成全部收起。

## 10b 共享交互组件（状态接口之后）

**Files:** 新 `src/components/WorkspacePanel.tsx` / `.test.tsx` / `.css`；将已存在的可见overlay判断从NativeLearningSurface原样移到 `src/lib/workspace.ts`，由原名称re-export保持播放器调用兼容，共享组件用较窄的modal/menu选择器以免把自身peek当对话框。

**Props:** `{label,side:'left'|'right',state:PanelState,limits:PanelLimits,tabs:{id,label,icon:ReactNode,content:ReactNode}[],onChange:(next:PanelState)=>void}`。现有reducer计算变化；onChange只报告状态，不自己写偏好。

- 根保留窄图标入口，当前aside仅closed时hidden。内容首次激活才挂载，之后保活且其他tab hidden；每个tab独立滚动/搜索。禁用未激活内容不读取所有远端/AI材料。
- 仅peek挂data-native-overlay；用既有Resize/MutationObserver更新原生视口。peek外点/Esc关闭；IME、输入焦点、defaultPrevented、可见dialog/menu优先，固定面板不被外点关闭。
- 固定面板有宽度separator，pointer capture跟踪横向增量，按左右侧处理方向；方向键每次10px，Home/双击复位，明确辅助说明和aria-valuemin/max/now。小窗口只CSS夹宽，不回写state。
- [x] RED→GREEN：未访问内容不挂载；切tab/收起保留搜索与滚动；peek外点/Esc、固定不收起、IME/输入/对话框优先；左右拖宽/越界/键盘/双击；原生overlay标记仅peek。追加关闭后焦点恢复、两侧Escape焦点优先先失败再修复。
- [x] 状态与组件连同现有学习区/偏好33项定向通过，完整前端404项/77文件通过，生产构建成功。只前端变更不重复Rust回归。证据 `docs/evidence/merge/task10-panel-foundation.md`；本步仍不替换真实工作区调用，10c再按现有保存/播放生命周期接入。
