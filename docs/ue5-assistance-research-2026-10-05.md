# 栈藏：UE5 环境静帧制作的外部辅助调研

日期：2026-10-05。仅调研与文档；未安装插件、修改程序或 UE 工程。用户已选定：环境静帧，首阶段只做外部辅助，重点是素材准备与导入前检查。以下优先级为产品判断，不是来源承诺。

## 首阶段建议

| 方向 | 最小增量 | 可验证边界 |
| --- | --- | --- |
| 贴图集与 RGBA 通道检查 | 材质贴图手动可修正分组、单通道灰度预览、尺寸/缺项提示、ORM 映射与 UE 导入说明 | 能检查源文件；不能证明最终 UE 材质视觉一致 |
| 资产命名和导入目录建议 | 根据用户选定资产类型生成前缀与建议名称、导出可阅读清单 | 首阶段提供建议，不直接重命名 UE .uasset |
| 外部模型检查 | 复用现有 Three.js 查看器展示模型统计、贴图关联；有需要再加官方 glTF Validator | 格式/结构检查不等于 UE 导入成功、Nanite 合格或运行性能验证 |

这些应挂在现有素材详情和导出流程，复用已有媒体对比/笔记链接，不建立另一套项目、任务或笔记模块。

### 1. 贴图集、ORM 通道预览与导入建议

- **已核实**：Epic 文档说明可把 AO、Roughness、Metallic 等遮罩放到不同颜色通道；打包遮罩应关闭 sRGB，并检查材质采样节点类型。它并未要求所有项目采用同一通道顺序。[Using Texture Masks](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-texture-masks-in-unreal-engine)。
- **独立实现建议**：默认提供 ORM（R=AO、G=Roughness、B=Metallic）模板，但明确这是可改约定；保存用户确认的通道映射。复用本项目已有图像与 Three.js 能力做预览，不嵌入完整材质编辑器。说明“缺少独立 Roughness 文件也可能是常量/已打包”，不能直接判为错误。
- **兼容性**：上述是材质输入建议，无插件二进制兼容问题。必须记录目标 UE 小版本，避免把当前文档显示的 5.8 视为用户版本或通用导入自动化保证。
- **未知**：本轮未测试 EXR、UDIM、16 位源文件解码/显示，未验证用户素材的色彩管理、法线方向和实际 UE 导入；首版应明确支持格式与未检查项。

### 2. 命名和目录规范

- **已核实**：Epic 推荐 `[AssetTypePrefix]_[AssetName]_[Descriptor]_[OptionalVariantLetterOrNumber]`，例如纹理 `T_`、静态网格 `SM_`、材质 `M_`、材质实例 `MI_`。这是建议，项目约定优先。[Epic Recommended Asset Naming Conventions](https://dev.epicgames.com/documentation/en-us/unreal-engine/recommended-asset-naming-conventions-in-unreal-engine-projects)。
- **可复用规则参考**：Allar/ue5-style-guide 为公开源码/文档，LICENSE 是 MIT，复制相关内容须保留版权和许可声明。它是社区规范，不能当作 Epic 强制校验器。[仓库](https://github.com/Allar/ue5-style-guide)、[LICENSE](https://github.com/Allar/ue5-style-guide/blob/main/LICENSE)。
- **维护快照**：GitHub API 查询 `archived=false`，`pushed_at=2023-10-05T18:32:26Z`；不据此推断它跟踪所有新 UE5 类型。[一手仓库元数据](https://api.github.com/repos/Allar/ue5-style-guide)。
- **实现判断**：独立实现少量可配置规则与建议清单即可，首阶段不安装社区 Linter。源 FBX/图片名称与 UE 导入后资产名需区别呈现。Epic 明确要求 .uasset 管理通过 UE Editor；磁盘上移动/重命名可能破坏引用。[Working with Assets](https://dev.epicgames.com/documentation/en-us/unreal-engine/working-with-assets-in-unreal-engine)。

### 3. 外部模型检查

- 前次已核查的 [Khronos glTF Validator](https://github.com/KhronosGroup/glTF-Validator) 与 Three.js 事实详见 [既有调研](./github-feature-research-2026-10-05.md)：可复用现有查看器，官方 glTF Validator 是按需集成候选，不能用它验证 FBX/.uasset。
- 首版建议只展示实际已加载模型的统计和发现的问题，不产出“UE 就绪/Nanite 通过”评分。UE 实际导入设置、材质、目标渲染器、碰撞和 Nanite 结果尚未在引擎内验证。

## 后续备选，不进入首阶段

### 4. 蓝图片段库

- **方向**：用户从 UE 复制节点文本，栈藏保存原文、引擎版本与依赖说明，提供检索和复制回 UE。能减少重复搭节点；不声称文本就包含完整变量、函数、插件和资源依赖。
- **参考与许可**：blueprintue-self-hosted-edition 有公开源码，主仓库 LICENSE 为 MIT；网站服务需要 PHP >=8.4、MySQL >=8 或 MariaDB >=10.6，SQLite 用于限流，不适合原样搬进离线 Tauri。[仓库 README](https://github.com/blueprintue/blueprintue-self-hosted-edition)、[许可证](https://github.com/blueprintue/blueprintue-self-hosted-edition/blob/main/LICENSE)。
- **维护与 UE 版本**：API `pushed_at=2026-07-18T19:25:55Z`，未归档；发布页最新 v4.3.0。v4.1.0 发布说明把默认 UE 版本改为 5.6，这只证明有该版本标签支持，不保证全部节点可在所有 UE5 小版本间往返。[API](https://api.github.com/repos/blueprintue/blueprintue-self-hosted-edition)、[发布记录](https://github.com/blueprintue/blueprintue-self-hosted-edition/releases)。
- **直接复用 vs 独立实现**：简单文本存储/复制可独立实现；可视节点预览应另作复用审计。源码树有 `www/bue-render/render.js` 与 CSS，但本轮下载版权头失败，未完成文件级许可、依赖和离线测试；不能把“主仓库 MIT”直接升级为“渲染器已确认可抽取打包”。[源码树](https://github.com/blueprintue/blueprintue-self-hosted-edition/tree/main/www/bue-render)。

### 5. 灯光/MRQ 配方与结果对照

- **已核实**：MRQ 已支持 Master、Shot 设置预设，且两者是不同资产类型；渲染队列也可保存为资产。因此栈藏的增量应是配方版本、目标 UE 版本、预设路径与实际渲染结果对照，不重复实现 UE 配置 UI。[Render Settings and Formats](https://dev.epicgames.com/documentation/en-us/unreal-engine/cinematic-render-settings-and-formats-in-unreal-engine)、[5.4 MRQ 文档](https://dev.epicgames.com/documentation/en-us/unreal-engine/render-cinematics-in-unreal-engine?application_version=5.4)。
- **兼容性/未知**：5.4 官方 MRG 文档显示 Experimental，不应把它和当前文档中的状态混用；首阶段不读写 .uasset，也不承诺灯光/曝光参数组合跨版本同样输出。若仅有几条文字说明，可直接使用现有笔记；只有形成“可复制参数差异 + 渲染 A/B”闭环才值得独立功能。[5.4 MRG](https://dev.epicgames.com/documentation/unreal-engine/movie-render-graph-in-unreal-engine?application_version=5.4)。

## UE editor bridge：未来可选能力与限制

- **官方能力**：Remote Control 可通过 HTTP/WebSocket 访问暴露的属性和函数；官方快速入门演示读取灯光旋转及调用事务化修改。插件要在工程启用并重启，服务显式启动；默认绑定 127.0.0.1。首阶段不启用。[Remote Control Quick Start](https://dev.epicgames.com/documentation/unreal-engine/remote-control-quick-start-for-unreal-engine?lang=en-US)。
- **集成判断**：若未来需要真实 UE 导入校验，优先评估官方编辑器 API/Python/Remote Control 的窄能力接口，不先造 C++ 插件。Epic 文档中的 API 能力不代表再分发其代码已获 MIT 授权；本轮未核查 Epic 源码再分发条款。[Scripting and Automating the Editor](https://dev.epicgames.com/documentation/en-us/unreal-engine/scripting-and-automating-the-unreal-editor)。
- **安全边界**：Epic 要求不要公开暴露端口；任意 Python 执行具有明显风险。后续若采用联动，应限定本机、限制命令并由用户确认工程变更；这是产品实现判断，不是本轮授权安装。[官方 Remote Control 安全设置](https://dev.epicgames.com/documentation/unreal-engine/API/Plugins/RemoteControlCommon/URemoteControlSettings?lang=en-US)。
- **高风险备选**：chongdashu/unreal-mcp 源码公开，README 声称 MIT，但根目录独立 LICENSE 未成功取得；实验项目，不建议生产使用，说明可能发生破坏性 API 变化。README 标明 UE5.5+、Python3.12+，需要编译 C++ 插件和 Python MCP 服务；功能涵盖 Actor 和蓝图创建/修改。不能把 README 的“5.5+”视为已测全部后续版本。[README](https://github.com/chongdashu/unreal-mcp)。API 维护快照 `pushed_at=2025-04-22T02:02:27Z`，未归档；没有验证版本矩阵、回滚或自动命令的安全性。[API](https://api.github.com/repos/chongdashu/unreal-mcp)。

## 下一步访谈需要确认

先收集一组真实材质/模型源文件及用户常见导入问题，再决定首版支持哪些格式、命名约定和贴图映射；不要用通用模型检查替代真正的 UE 验证。本轮未修改任何 UE 工程，也未对用户素材做引擎测试。
