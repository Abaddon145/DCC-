# 栈藏 DCC 素材库：GitHub 功能参考调研

调研日期：2026-10-05。目标：个人 UE5/CG 创作，Tauri 2 + React + Rust 的 Windows 本地应用。本次只做推荐，没有安装软件或改动程序。Star 为查询时快照或 GitHub 页面舍入值，表示关注度，不表示稳定性；维护判断另看发布与代码。

现有素材导入、精确查重、独立媒体库、Three.js 查看器、参考板、知识库反向链接/复习、AI、工作流模板、A/B 对比、采集扩展、任务中心及备份恢复均不重复推荐。已核对 package.json 与 src-tauri/Cargo.toml：现有 Three.js 0.186、rusqlite bundled-full、image 0.25、sha2；以下方案先复用这些能力。

## 建议的五个增量功能

| 优先级 | 功能 | 对日常创作的价值 | 最小落地范围 |
| --- | --- | --- | --- |
| P1 | 材质贴图集与通道预览 | 快速知道一组材质缺什么、贴图之间是否配套 | BaseColor/Normal/Roughness/Metallic/AO/Height 手动可修正分组；尺寸/格式检查；R/G/B/A 单通道预览；导出导入清单 |
| P1 | 跨模块统一搜索 | 一个入口找素材、媒体、笔记、项目与镜头任务 | 结果显示类型、命中摘要、路径，点击跳到原模块；先搜已有元数据与笔记文字 |
| P1 | 模型检查卡片 | 导入 UE 前发现模型结构或缺失贴图问题 | 已加载模型的面数/材质/纹理尺寸；GLB/glTF 可选校验报告；明确错误、警告和未检查项 |
| P2 | 参考图局部标注 | 把“这个转角/色调/布光值得学”固定在图片上 | 矩形/箭头 + 短注释，坐标跟随图片缩放；持久化为旁路 JSON，不修改原图 |
| P2 | 本地相似图候选 | 找同图不同分辨率、压缩版本和部分近似图片 | 缩略图感知哈希、可调阈值、候选分组，用户逐项确认；不自动删除 |

以上排序是结合本项目现状的判断，不是这些开源项目的官方承诺。推荐先完成 P1，再按真实使用频率决定 P2。

## 七个候选的一手来源核对

### 1. Material Maker：贴图集和 UE 导入说明的参考，P1

- 热度与维护：5,962 stars；REST 元数据 pushed_at 为 2026-10-03；发布页最新条目为 1.7。基于 Godot 的程序化纹理与模型绘制工具，代码默认 MIT，个别文件可有单独声明。[仓库/许可证](https://github.com/RodZill4/material-maker)、[发布记录](https://github.com/RodZill4/material-maker/releases)。
- 直接集成还是借鉴：**借鉴贴图输出与材质预览思路**。不要为素材库嵌入 Godot 或完整节点材质编辑器；本项目现有 image 与 Three.js 足以做贴图信息、通道预览与基础 PBR 球预览。
- UE 适配边界：官方文档的 UE5 导出会生成 Python、纹理和 shader，再由用户在 UE 中执行。当前建议只生成可阅读的贴图/通道/路径清单，不宣称自动生成 UE 材质已验证。[官方导出说明](https://rodzill4.github.io/material-maker/doc/export.html)。
- 风险：文件命名只能给建议，通道打包约定应允许手动修改；缺失通道提示不是“材质一定错误”；不要把缺少贴图和刻意使用常量混为一谈。

### 2. Omnisearch：统一搜索交互参考，P1

- 热度与维护：2,159 stars；REST 元数据 pushed_at 为 2026-09-19；发布页最新 1.31.0，说明修复文件增删竞态和索引内存保留问题。[仓库](https://github.com/scambier/obsidian-omnisearch)、[发布记录](https://github.com/scambier/obsidian-omnisearch/releases)。
- 许可证：GPL-3.0，**只借鉴产品交互，不复制插件代码**。README 明确使用 MiniSearch，支持文件名/标题权重、模糊检索和键盘导航；中文支持还依赖额外插件，所以不能把“搬过来即可搜中文”当作既定事实。[README 和 LICENSE](https://github.com/scambier/obsidian-omnisearch)。
- 最小适配：复用现有素材、媒体和知识库的 SQLite FTS5（trigram）查询，统一结果类型与跳转；标题/标签优先、摘要辅助。先测中文短词与跨模块结果排序，不急着引入另一套独立全文索引、OCR、向量数据库或搜索服务器。项目现有索引可见 `src-tauri/src/schema.sql` 与 `notes_schema.sql`。
- 风险：数据库全文检索不是自动具有中文词语分词；要保留中文短词可用的查询回退。更新、删除、回收站操作都必须更新搜索结果，防止命中已删除内容。[SQLite FTS5 官方文档](https://www.sqlite.org/fts5.html)。

### 3. Khronos glTF Validator：可直接集成的模型诊断，P1

- 热度与维护：471 stars，Apache-2.0；REST 元数据 pushed_at 为 2026-09-18。关注度不算高，但属于格式标准组织维护的专项工具；仓库有自动化测试并列出支持的扩展，值得按专项能力评估。[仓库/许可证](https://github.com/KhronosGroup/glTF-Validator)、[代码历史](https://github.com/KhronosGroup/glTF-Validator/commits/main/)。
- 功能证据：对 glTF 2.0/GLB 结构、buffer/accessor、图像及已支持扩展进行验证，输出问题和统计 JSON。[README](https://github.com/KhronosGroup/glTF-Validator)、[问题码列表](https://github.com/KhronosGroup/glTF-Validator/blob/main/ISSUES.md)。
- 集成建议：**有明确需求后直接集成官方 JS 包**，先按需检查自包含 GLB，可放 Worker 并限制文件大小/输出条数。官方提供浏览器 validateBytes 与 externalResourceFunction；分离式 glTF 引用资源必须经本应用安全的相对路径解析，不能照搬 Node 示例开放任意磁盘读权限。[官方 JS 接口](https://github.com/KhronosGroup/glTF-Validator/blob/main/node/README.md)。
- 风险：这是 glTF 检查器，不覆盖 FBX、UE .uasset，也不证明某个 UE 版本导入效果正确；报告必须显示“本工具未支持的扩展”，避免把未知当通过。

### 4. Three.js：复用已安装依赖，做模型统计和材质预览，P1

- 热度与维护：116,242 stars，MIT；发布页最新 r186，日期显示 9 月 24 日，本项目已使用 0.186。[仓库/许可证](https://github.com/mrdoob/three.js)、[发布记录](https://github.com/mrdoob/three.js/releases)。
- 集成方式：**直接复用现有 Three.js**，不再加一套 3D 引擎。遍历已加载 scene，读取几何、材质与贴图数据；基础 PBR 预览复用 MeshStandardMaterial。[BufferGeometry 实际代码](https://github.com/mrdoob/three.js/blob/r186/src/core/BufferGeometry.js)、[官方材质文档](https://threejs.org/docs/#api/en/materials/MeshStandardMaterial)。
- 风险：每帧 renderer.info.render.triangles 不是稳定的资产总面数，受当前可见物体、绘制次数影响；统计需要明确区分共享几何与实例数，且不能将点/线图元都除以三。Three.js 的 PBR 预览只能用于检查，不能承诺与 UE 渲染完全一致。

### 5. Annotorious：图片局部标注备选，P2

- 热度与维护：867 stars；BSD-3-Clause；发布页最新 v3.4.0，包括多边形节点编辑改进。当前正确仓库是 annotorious/annotorious，避免与旧组织地址/旧版本混淆。[仓库/许可证](https://github.com/annotorious/annotorious)、[发布记录](https://github.com/annotorious/annotorious/releases)。
- 直接集成还是借鉴：**简单标注先用原生 SVG，复杂多边形编辑才考虑直接集成**。官方 API 支持从 JSON 加载标注和 createAnnotation 事件，具有 React 相关包。[README](https://github.com/annotorious/annotorious)。
- 风险：参考板整体拖拽、图片缩放与标注选区会争夺 pointer 事件；标注必须存图片自身坐标，不能存屏幕坐标。旋转/裁切后需要定义坐标变换；默认不烧录进原始图片。

### 6. Czkawka：相似图分组的 Rust 参考，P2

- 热度与维护：33,910 stars；发布页 12.0.2 明确日期为 2026-09-09。支持相似图、缓存和可复用 Core；旧 GTK 前端 12.0 为最后版本，持续开发方向为 Krokiet/Core。[仓库](https://github.com/qarmin/czkawka)、[发布说明](https://github.com/qarmin/czkawka/releases)。
- 许可证分组件：Core 的 Cargo.toml 明确 MIT；README 写 GTK/CLI 为 MIT，Krokiet/Cedinia 为 GPL-3.0-only，图片/音频资源 CC BY 4.0。不能仅凭 GitHub API 的 NOASSERTION 将全项目当作无许可，也不能将所有 GUI 都当 MIT。[Core manifest](https://github.com/qarmin/czkawka/blob/master/czkawka_core/Cargo.toml)、[README 许可证段落](https://github.com/qarmin/czkawka#license)。
- 集成建议：**借鉴候选分组、缓存、人工确认的流程**；先用已安装 image 从缩略图算轻量 dHash，不引 Python/CLIP。完整 Core 当前要求 Rust 1.94.1，本项目声明 1.89，且 Core 带音频、PDF、压缩包等广泛依赖；为了单个相似图功能整包引入不划算。[Core manifest](https://github.com/qarmin/czkawka/blob/master/czkawka_core/Cargo.toml)、[相似图源码](https://github.com/qarmin/czkawka/tree/master/czkawka_core/src/tools/similar_images)。
- 风险：感知哈希会误报，裁切、旋转和语义相似不是简单 dHash 的可靠能力；只提供候选和阈值，保留现有 SHA 精确查重，不自动删除。

### 7. SingleFile：采集可靠性参考；不推荐整包嵌入

- 热度与事实：约 22.5k stars，AGPL-3.0；现有仓库有测试、完整归档实现及已知问题文档。这里只确认产品能力与代码规模，没有取得其精确最新版本/发布时间，因此不以“持续高频发布”替它背书。[仓库/许可证](https://github.com/gildas-lormeau/SingleFile)。
- 不推荐原因：保存完整网页是另一项产品能力，本项目已有采集扩展；AGPL 代码和完整资源归档管线不适合顺手复制进现有应用。**仅借鉴取消、逐项进度、错误说明和批量任务的交互**。
- 适配方向：给既有采集任务保留待处理/处理中/成功/失败状态、错误原因和重试入口；让失败可恢复比增加完整网页归档更有近期价值。这是对现有功能的优化，不另列为新模块。
- 浏览器限制事实：官方明确列出 canvas/视频截取、部分 iframe、浏览器受保护域名等限制；可重试不代表能够绕过这些限制，失败提示应区分临时网络错误与浏览器权限限制。[官方已知问题](https://github.com/gildas-lormeau/SingleFile/blob/master/known-issues.md)。

## 实施顺序判断

先修复主审查发现的媒体大文件读取和查看器性能问题，再增加贴图集、统一搜索与检查卡片，避免功能越多越卡。材质检查清单与参考标注可以连到已有项目/镜头任务，维持当前应用结构；无需另做知识库系统、完整材质编辑器或自动操作 UE 工程的插件。

本文维护日期若来自 REST 元数据，指仓库 pushed_at，不能等同于主分支最新功能提交；部分 GitHub 网页是缓存或舍入展示。准备真实集成时仍需锁定具体版本、核对该版本许可证与依赖，而不能直接跟随 master。
