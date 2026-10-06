# UE 工程素材预览：能力与边界

调查日期：2026-10-05。范围：只读安装记录与一手文档；未读取用户工程或资产、启动 UE、安装或启用插件。本文是可行性调查，不是已实现功能或项目兼容性验证。

## 本机安装记录

来源：`C:\ProgramData\Epic\UnrealEngineLauncher\LauncherInstalled.dat` 中的 UE 条目。

| 条目 | AppVersion | InstallLocation |
| --- | --- | --- |
| UE_5.6 | 5.6.1-44394996+++UE5+Release-5.6-Windows | C:\Program Files\Epic Games\UE_5.6 |
| UE_5.7 | 5.7.4-51494982+++UE5+Release-5.7-Windows | C:\Program Files\Epic Games\UE_5.7 |
| UE_5.8 | 5.8.2-56702186+++UE5+Release-5.8-Windows | C:\Program Files\Epic Games\UE_5.8 |

`HKCU\Software\Epic Games\Unreal Engine\Builds` 不存在。`HKLM\SOFTWARE\EpicGames\Unreal Engine` 有 5.3、5.4、5.6、5.7 的 `InstalledDirectory`，均指向 `C:\Program Files\Epic Games\UE_版本`；WOW6432Node 同名位置有历史 4.0 条目。记录可能残留，未核实可执行文件存在、引擎可启动或具体工程所需版本。来源：本次只读注册表查询。

## 两层预览能力

| 用户看到的能力 | 官方基础 | 实际边界 |
| --- | --- | --- |
| 工程素材目录、名称、类型、标签 | Asset Registry 读取未加载资产的头部元数据，按路径/类型过滤 | 这是 UE 子系统；不代表栈藏的 JavaScript/Rust 已能解析任意 `.uasset`。异步扫描须等待完成。 |
| 资产缩略图 | `ThumbnailTools::RenderThumbnail` 在 UnrealEd 模块中，接收 UObject 和渲染目标/缩略图数据 | 这是 C++ 编辑器 API。纯 Python 的统一缩略图导出绑定、各资产类型渲染器与无窗口渲染条件，尚未实测。不能承诺所有类型都支持。 |
| 静态网格旋转/缩放预览 | UE GLTFExporter 将加载的 UStaticMesh 导出 `.glb`，由现有 Three 预览消费 | 这是导出的代理模型；UE 材质、光照、Nanite 与执行逻辑不随之获得原生运行能力。 |

来源：[Asset Registry](https://dev.epicgames.com/documentation/en-us/unreal-engine/asset-registry-in-unreal-engine)、[ThumbnailTools::RenderThumbnail（UE5.5 API）](https://dev.epicgames.com/documentation/unreal-engine/API/Editor/UnrealEd/ThumbnailTools__RenderThumbnail?application_version=5.5)、[UnrealEd 模块（UE5.8，仍列出该接口）](https://dev.epicgames.com/documentation/unreal-engine/API/Editor/UnrealEd?lang=en-US)、[GLTFExporter Python API（UE5.6）](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/GLTFExporter?application_version=5.6)。5.6 Python API 明列 UMaterialInterface、UStaticMesh、USkeletalMesh、UWorld、UAnimSequence、ULevelSequence、ULevelVariantSets；不能据此推断任意 UObject/Blueprint 可直接导出。

## Three.js 代理的保真边界

- UE glTF 导出仅表达支持的模型与材质信息。复杂材质依赖表达式匹配或烘焙；时间、相机位置、反射方向等动态表达式会变成导出时的静态结果。静态网格导出一个 LOD，碰撞几何被忽略。来源：[官方 glTF 内容处理说明](https://dev.epicgames.com/documentation/en-us/unreal-engine/how-the-gltf-exporter-handles-unreal-engine-content)。
- Nanite 采用 UE 内部网格格式、分簇流送和专用渲染路径。判断：普通 GLB/Three 预览只能展示可导出的三角网格，不能执行 UE Nanite；具体版本使用原网格还是某个替代网格及最终细节，须样本验证，不能预先保证。来源：[Nanite 官方说明](https://dev.epicgames.com/documentation/en-us/unreal-engine/nanite-virtualized-geometry-in-unreal-engine)。
- Blueprint 是 UE 的类、构造与事件逻辑系统。判断：导出的模型/场景快照不等于运行 Blueprint；模型旋转预览不能验证蓝图行为。来源：[Blueprint 官方介绍](https://dev.epicgames.com/documentation/en-us/unreal-engine/introduction-to-blueprints-visual-scripting-in-unreal-engine)及上面的 GLTFExporter 支持类型表。
- 若验收要求 UE 原生材质或行为，需另行设计 UE 本身渲染/运行、图像或视频缓存、或交互画面传输。此方向的保真度、性能和窗口整合均未验证，不能承诺 Exact 渲染。该项是基于以上能力边界的设计推论。

## UE Python 缓存生成的要求

官方 Python 是编辑器内环境，不能用系统 Python 单独加载 `.uasset`。必须为指定工程启用内置 **Python Editor Script Plugin**，启用后重启；不必额外安装系统 Python。官方完整编辑器 `-ExecutePythonScript` 方式还要求 **Editor Scripting Utilities**；`-run=pythonscript -script=...` 可无编辑器 UI 运行，但不自动加载关卡。不能由“支持无窗口 Python”推断“全部缩略图及材质烘焙能在任何 commandlet/无 GPU 环境执行”。来源：[官方 Python 配置与命令行说明](https://dev.epicgames.com/documentation/en-us/unreal-engine/scripting-the-unreal-editor-using-python)。

GLTFExporter 已有可调用的 Python 导出接口；必须在对应工程中实际可用。5.6 已有该接口，不能未经验证将其签名及成功率推广到全部历史版本。[UE5.6 Python API](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/GLTFExporter?application_version=5.6)、[UE C++ 导出接口](https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Plugins/GLTFExporter/UGLTFExporter)。

落地动作包括启动相容引擎、读取指定工程素材、可能加载其代码/依赖插件、写入栈藏缓存；启用内置插件也属于工程配置变更。仅有“不安装自研/第三方插件”的约束，未必排斥内置 Python；若之前“不装 UE 插件”也包含禁止启用内置插件，Python 路线就超出该边界。具体工程 EngineAssociation、插件依赖、当前插件启用状态、缓存目录写权限与加载成功率均未知；本次未触碰工程核实。

## FModel / CUE4Parse 范围与许可

FModel 官方自述是 UE 游戏归档浏览器；其 Getting Started 明确围绕游戏 archives，且不直接载入导出的游戏 packages。其 Snooper 渲染器自述针对 **cooked UE packages**。因此把它作为任意未烘焙工程原生 `.uasset` 的保证方案缺乏依据。来源：[FModel README 与 GPL-3.0 许可](https://github.com/4sval/FModel)、[官方 Getting Started](https://github.com/4sval/FModel/wiki/Getting-Started)、[Snooper 范围](https://github.com/FModel/Snooper)。

CUE4Parse 是 UE4/UE5 archives/packages 的 C# 解析库，示例使用明确的游戏/引擎版本配置，能解析/转换多种类；许可为 **Apache-2.0**，另有第三方 NOTICE。不能笼统断言它“完全不能读未烘焙文件”，也不能把库的存在当作“所有版本、任意工程自定义类/插件资产与依赖可忠实预览”的承诺。其 README 未给出这一通用保证。来源：[CUE4Parse README、LICENSE 与 NOTICE](https://github.com/FabianFG/CUE4Parse)。FModel 与 CUE4Parse 的许可分别评估，不能把 FModel 当成 Apache 库直接集成。

## 建议的最小边界（待用户决定）

先明确“预览”是否接受 **目录 + 缩略图 + 静态网格 GLB 代理**，再确认是否允许启动 UE 并启用其内置脚本能力。若允许，可先用用户指定的单工程做最小导出验证，复用现有 Three 查看器；缩略图绑定与渲染条件另行验证，失败时明确显示原因。若要求全类型原生行为或 UE 渲染结果，范围须独立定义。上述建议尚未实施。

## GitHub 复用调查（2026-10-06 补充）

本轮已确定的产品范围：允许 UE 工具；首批静态网格、纹理、材质、材质实例；网格 GLB 交互预览、材质静态图、纹理放大；手动刷新缓存，不随栈藏启动 UE；先目录与缩略图，模型按需生成。仅查看公开源码、许可证与维护记录，未下载、安装、编译或运行候选。

**建议直接复用 Kura 的缩略图小组件，加现有 UE 官方 GLTFExporter；不引入另一套完整资产库。** 没有查到可以直接嵌入栈藏、同时保证任意未烘焙 UE5 工程和本机全部引擎版本的现成完整方案。以下项目能力以其文档与具体源码为依据，星数不构成兼容性证据。

| 候选 | 许可、版本与维护记录 | 已核实能力 | 集成判断 |
| --- | --- | --- | --- |
| [Yashashin/Kura](https://github.com/Yashashin/Kura) | MIT；C++ 编辑器插件 + Python 工作流；uplugin 1.1.0；固定提交日期 2026-08-27 | 完整个人 UE 资产库；可提取缩略图 C++ 工具，加载工程 UObject、RenderThumbnail 后写 PNG；专门的材质预览入口 | **优先取缩略图组件。** 不取上传/导入/迁移/库 UI；四类索引使用官方 Asset Registry。GLB 导出入口未在此候选中证实。 |
| [munucrafts/CPP-UE-GetThumbnails](https://github.com/munucrafts/CPP-UE-GetThumbnails) | MIT；C++；uplugin 1.0；README 5.3+；最后 push 2026-02-21 | `SaveThumbnail(FAssetData)` 调用 `LoadThumbnailsFromPackage`，把包中已有缩略图编码为 PNG；并非重新渲染每类资产 | 小而可参考，但源码直接解引用缺失缩略图，未检查编码/写入结果；文件名只有 AssetName，有同名覆盖风险。不能整包直接作为可靠生成管线。 |
| [ZUPTI/EasyThumbnailGenerator](https://github.com/ZUPTI/EasyThumbnailGenerator) | MIT；C++ Editor-only；1.4.0；明确目标 UE5.8，更早版本不维护；最后 push 2026-09-29 | 静态/骨骼网格透明 PNG、相机/灯光预设、批量输出 | 四类中的纹理、独立材质/材质实例没有已证实的专用导出支持；需要额外 UI 与渲染管线，也不覆盖本机 5.6/5.7 的维护承诺。暂不整包引入。 |
| [Andrew-modeler/BlenderAssetBrowser](https://github.com/Andrew-modeler/BlenderAssetBrowser) | 插件子目录 LICENSE 为 Apache-2.0；C++ UE 插件，仓库含 C 第三方源码；README 5.3+，仅列 5.6 构建初始化日志；最后 push 2026-05-11 | UE 工程内跨项目库、AssetPreview 缩略图缓存；README 同时列 SQLite、ONNX/SigLIP2、Assimp、Blender bridge | 最接近完整 UE Asset Browser，但库、搜索与 UI 重叠，第三方依赖多；本轮未核实四类外部 PNG/GLB 批量导出的可调用入口。启动日志不证明预览全部可用。暂不整包引入。 |
| [Prikalel/unreal-assets-to-glb](https://github.com/Prikalel/unreal-assets-to-glb) | GPL-3.0；Python 3.10+；默认 5.5 分支，README 包版本 5.5.0 / 4.27.2.0；最后 push 2026-06-20 | **明确处理 editor-time、未烘焙工程**；不依赖 UE 安装；静态网格 GLB、BaseColor PNG、材质父链/实例纹理覆盖解析 | 不能误归为 cooked 游戏工具。但明确限 UE5.5/4.27.2，不覆盖本机 5.6–5.8；README 排除 PBR、Nanite、材质烘焙等。GPL 与新增 Python 解析依赖也须独立评估；不作为当前默认集成。 |

维护时间来源：本轮读取 [Kura 固定提交](https://github.com/Yashashin/Kura/commit/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458)及 GitHub repository API 的 `pushed_at`：[CPP-UE-GetThumbnails](https://api.github.com/repos/munucrafts/CPP-UE-GetThumbnails)、[EasyThumbnailGenerator](https://api.github.com/repos/ZUPTI/EasyThumbnailGenerator)、[BlenderAssetBrowser](https://api.github.com/repos/Andrew-modeler/BlenderAssetBrowser)、[unreal-assets-to-glb](https://api.github.com/repos/Prikalel/unreal-assets-to-glb)。`pushed_at` 只是最后推送时间，不等于正式发布或兼容性测试。

许可逐一查看正文：[Kura MIT](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/LICENSE)、[CPP MIT](https://github.com/munucrafts/CPP-UE-GetThumbnails/blob/main/LICENSE)、[Easy MIT](https://github.com/ZUPTI/EasyThumbnailGenerator/blob/main/LICENSE)、[BlenderAssetBrowser Apache-2.0](https://github.com/Andrew-modeler/BlenderAssetBrowser/blob/main/BlenderAssetBrowser/LICENSE)、[unreal-assets-to-glb GPL-3.0](https://github.com/Prikalel/unreal-assets-to-glb/blob/5.5/LICENSE.md)。BlenderAssetBrowser 根目录 GitHub 元数据没识别到 license，不能据此说无许可证；正文在插件子目录。上述开源组件许可不改变 Unreal Engine 自身许可。

### 可提取的 Kura 缩略图组件

固定仓库提交：`f8f6d06bf2a626f94cb10b0104b71ce80c6c5458`（2026-08-27）。不是浮动 main，也不是只按版本名猜测。

- [Kura/Source/Kura/Private/KuraThumbnailLibrary.cpp](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Private/KuraThumbnailLibrary.cpp)：实际渲染、编译/流送等待、PNG 输出、空结果及写入失败检查；纹理收集分支处理 UTexture、UMaterialInterface 和 UStaticMesh，UMaterialInterface 包含材质及材质实例。
- [Kura/Source/Kura/Private/KuraThumbnailLibrary.h](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Private/KuraThumbnailLibrary.h)：`UKuraThumbnailLibrary : UBlueprintFunctionLibrary`，暴露 `ExportThumbnailPng` 与 `ExportMaterialThumbnailPng`；源文件说明经 Python UFUNCTION bridge 调用。
- [Kura.Build.cs](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Kura.Build.cs)与 [Kura.uplugin](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Kura.uplugin)：原插件是 Editor 模块，启用 PythonScriptPlugin、EditorScriptingUtilities；原 Build.cs 含 UnrealEd、ImageCore 等依赖。抽取后应只保留新导出模块实际需要的依赖，不能照搬库 UI 依赖。

两份缩略图源约 12 KB，适合放入最小 UE Editor 导出模块；需要保留 MIT 版权与许可文本，重新生成反射代码并按目标引擎构建。源码有 5.7 前后 GetUsedTextures API 分支，但这不是已完成 5.6/5.7/5.8 本机编译验证的证据。资产自身无可用 ThumbnailRenderer、编译失败、GPU/渲染环境或第三方类加载失败仍可能使导出失败。

官方 **GLTFExporter** 的 `unreal.GLTFExporter.export_to_gltf` 已可导出静态网格为 GLB，属于已有原生能力，无须另选第三方 GLB 库。它由 Unreal Engine 许可覆盖，不能把 Epic 源码标成 MIT/Apache 开源后独立打包。优先调用本机引擎提供的接口。[官方 UE5.6 Python API](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/GLTFExporter?application_version=5.6)。

推荐组合的实现成本判断：Asset Registry 索引脚本 + 上述两份缩略图组件及最小 Editor 模块 + 官方 GLTFExporter + 现有 Three 查看器。直接组件复用已找到；本轮仍只证明**源码方案可行**，没有编译、引擎运行、用户工程验证或性能测量结果，不能将其描述为可立即安装且保证成功的完整产品。
