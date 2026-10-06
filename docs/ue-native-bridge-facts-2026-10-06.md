# UE 原生资产库：本机与 Editor API 事实核查

调查日期：2026-10-06。范围：本机引擎安装文件、构建工具元数据、引擎头文件与必要实现、一手 Epic 文档、Kura 固定提交。本文研究过程未启动 UE、编译、启用插件、修改引擎/工程配置、读取用户工程或素材库；仅新增本报告。主任务另行进行的构建不算本报告的运行验证。

范围已确定为 StaticMesh、Texture、Material、MaterialInstance；UE 生成缩略图，独立 UE 工程打开原生资产编辑器，Asset Registry 索引，确认依赖后迁移。此前 [2026-10-05 可行性报告](<C:/Users/14582/Documents/ChatGPT/软件开发/docs/ue-project-preview-feasibility-2026-10-05.md>) 中的 GLB/Three 组合是旧方案，原生预览不需要 GLTFExporter。

标记：**已核实**表示本轮读到实际文件、声明或实现；**推断**表示这些事实支持的最小实现选择；**未验证**表示尚无本报告自己的构建、引擎运行或样本结果。

## 1. 安装记录对应真实编辑器

**已核实：三个安装目录、UnrealEditor.exe、UnrealEditor-Cmd.exe、Build.version 均存在。** PE 的 FileVersion/ProductVersion 与 Build.version 的分支、CL 相符，不只是 Launcher 残留记录。来源：[LauncherInstalled.dat](<C:/ProgramData/Epic/UnrealEngineLauncher/LauncherInstalled.dat>)；以下每行列出实际文件。

| 引擎 | Build.version | PE 版本 | 实际文件 |
| --- | --- | --- | --- |
| 5.6.1 | CL 44394996；CompatibleChangelist 43139311 | ++UE5+Release-5.6-CL-44394996 | [Editor](<C:/Program Files/Epic Games/UE_5.6/Engine/Binaries/Win64/UnrealEditor.exe>)、[Cmd](<C:/Program Files/Epic Games/UE_5.6/Engine/Binaries/Win64/UnrealEditor-Cmd.exe>)、[Build.version](<C:/Program Files/Epic Games/UE_5.6/Engine/Build/Build.version>) |
| 5.7.4 | CL 51494982；CompatibleChangelist 47537391 | ++UE5+Release-5.7-CL-51494982 | [Editor](<C:/Program Files/Epic Games/UE_5.7/Engine/Binaries/Win64/UnrealEditor.exe>)、[Cmd](<C:/Program Files/Epic Games/UE_5.7/Engine/Binaries/Win64/UnrealEditor-Cmd.exe>)、[Build.version](<C:/Program Files/Epic Games/UE_5.7/Engine/Build/Build.version>) |
| 5.8.2 | CL 56702186；CompatibleChangelist 55116800 | ++UE5+Release-5.8-CL-56702186 | [Editor](<C:/Program Files/Epic Games/UE_5.8/Engine/Binaries/Win64/UnrealEditor.exe>)、[Cmd](<C:/Program Files/Epic Games/UE_5.8/Engine/Binaries/Win64/UnrealEditor-Cmd.exe>)、[Build.version](<C:/Program Files/Epic Games/UE_5.8/Engine/Build/Build.version>) |

**未验证：**文件存在不证明编辑器能启动、目标素材的引擎版本相容、GPU 渲染可用或用户工程的插件可加载。以上检查只解决“是否真正安装”的问题。来源：上述文件检查范围。

## 2. C++ 构建前提

| 已核实项目 | 结果 | 主来源 |
| --- | --- | --- |
| VS 安装 | Visual Studio 2022 BuildTools 17.14.41；installationVersion 17.14.37710.0；isComplete/isLaunchable true；不需 VS IDE 才能使用其编译器 | [vswhere.exe](<C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe>) 的 `-all -products * -format json` 与 `-requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64` 查询；[Epic 的 Build Tools 配置说明](https://dev.epicgames.com/documentation/en-us/unreal-engine/setting-up-visual-studio-code-for-unreal-engine) |
| 编译器与链接器 | MSVC 目录族为 14.44.35207，但 cl.exe 产品版本为 **14.44.35229.0**、文件版本 19.44.35229.0；link.exe 为 14.44.35229.0 | [cl.exe](<C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/cl.exe>)、[link.exe](<C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/link.exe>) 的 VersionInfo |
| MSVC 库、MSBuild | vector 头、x64 libcmt.lib、MSBuild.exe 实文件存在 | [vector](<C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207/include/vector>)、[libcmt.lib](<C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207/lib/x64/libcmt.lib>)、[MSBuild.exe](<C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/MSBuild/Current/Bin/MSBuild.exe>) |
| Windows SDK | 10.0.26100.0 的 Windows.h、x64 kernel32.lib、ucrt.lib 存在 | [Windows.h](<C:/Program Files (x86)/Windows Kits/10/Include/10.0.26100.0/um/Windows.h>)、[kernel32.lib](<C:/Program Files (x86)/Windows Kits/10/Lib/10.0.26100.0/um/x64/kernel32.lib>)、[ucrt.lib](<C:/Program Files (x86)/Windows Kits/10/Lib/10.0.26100.0/ucrt/x64/ucrt.lib>) |
| UBT 和自带 .NET | 三版本均有 UnrealBuildTool.dll 与 Build.bat；5.6 自带 8.0.300/win-x64/dotnet.exe，5.7 自带 8.0.412，5.8 自带 10.0；UBT runtimeconfig 分别为 net8.0、net8.0、net10.0 | [5.6 UBT runtimeconfig](<C:/Program Files/Epic Games/UE_5.6/Engine/Binaries/DotNET/UnrealBuildTool/UnrealBuildTool.runtimeconfig.json>)、[5.7 同文件](<C:/Program Files/Epic Games/UE_5.7/Engine/Binaries/DotNET/UnrealBuildTool/UnrealBuildTool.runtimeconfig.json>)、[5.8 同文件](<C:/Program Files/Epic Games/UE_5.8/Engine/Binaries/DotNET/UnrealBuildTool/UnrealBuildTool.runtimeconfig.json>)、[5.8 dotnet.exe](<C:/Program Files/Epic Games/UE_5.8/Engine/Binaries/ThirdParty/DotNet/10.0/win-x64/dotnet.exe>)、[5.8 Build.bat](<C:/Program Files/Epic Games/UE_5.8/Engine/Build/BatchFiles/Build.bat>) |

**已核实：本机实际 MSVC 不落在本机 UE 的禁用段。** 5.7/5.8 配置禁止 14.44.0–14.44.35210；14.44.35229 高于这个范围。UBT 用 cl.exe 的 ProductVersion 判断禁用段，用工具链目录版本判断 preferred family，因此不能把目录名 14.44.35207 当成实际有缺陷的编译器版本。5.6 preferred 为 14.38.33130–14.38.99999；其 minimum 是 14.38.33130，14.44 不在该配置的禁用段。来源：[5.6 Windows_SDK.json](<C:/Program Files/Epic Games/UE_5.6/Engine/Config/Windows/Windows_SDK.json>)、[5.7 配置](<C:/Program Files/Epic Games/UE_5.7/Engine/Config/Windows/Windows_SDK.json>)、[5.8 配置](<C:/Program Files/Epic Games/UE_5.8/Engine/Config/Windows/Windows_SDK.json>)、[UBT 的版本判定](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Programs/UnrealBuildTool/Platform/Windows/MicrosoftPlatformSDK.cs:1144>) 与 [读取 cl.exe ProductVersion](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Programs/UnrealBuildTool/Platform/Windows/MicrosoftPlatformSDK.cs:1399>)。

**已核实：本机引擎配置与当前网页并不完全相同。** 当前 Epic 文档列 5.8 的 VS2022 最低 17.14、MSVC 最低 14.38、SDK 最低 22621/推荐 26100；本机 5.8 Windows_SDK.json 的 SDK MainVersion 是 22621、MinVersion 是 19041，允许上限 10.9.99999.0。本机 26100 在允许区间内。实际构建应依据该安装版本的配置；网页推荐不能替代 UBT 的检查。来源：[Epic VS 配置表](https://dev.epicgames.com/documentation/en-us/unreal-engine/setting-up-visual-studio-development-environment-for-cplusplus-projects-in-unreal-engine)、[本机 5.8 配置](<C:/Program Files/Epic Games/UE_5.8/Engine/Config/Windows/Windows_SDK.json>)。

**推断：**本机具备构建小型 Editor 插件的文件和工具链前提，无须先安装另一套 IDE/SDK。**未验证：**UHT/UBT/链接成功、各版本 ABI、插件二进制及 UE 实际运行。

### 默认启用的 Connecter 环境风险

**已核实：5.6/5.7 的引擎目录都装有默认启用的第三方 Connecter 插件，5.8 的 Engine/Plugins 下未找到同名描述文件。** 实际插件名是 `ConnecterUEPlugin`，目录名才是 `Connecter`。5.6 VersionName 0.9.5，5.7 1.0；均 `EnabledByDefault:true`、`Installed:false`、`CanContainContent:true`，模块 `ConnecterUEPlugin` 为 Editor/Default。来源：[5.6 ConnecterUEPlugin.uplugin](<C:/Program Files/Epic Games/UE_5.6/Engine/Plugins/Connecter/ConnecterUEPlugin.uplugin>)、[5.7 描述文件](<C:/Program Files/Epic Games/UE_5.7/Engine/Plugins/Connecter/ConnecterUEPlugin.uplugin>)，以及本轮 `rg --files` 对三个 Engine/Plugins 的描述文件查询。

**已核实：这两份安装都有 Source/ConnecterUEPlugin/ConnecterUEPlugin.Build.cs 和 Win64 UnrealEditor-ConnecterUEPlugin.dll / UnrealEditor.modules。** 5.6 Build.cs 定义 `public class ConnecterUEPlugin : ModuleRules`，类名正确；modules 的 BuildId 分别是 43139311/47537391。仅从 `Expecting to find ... module rules named ConnecterUEPlugin in UE5Rules` 不能断言 Build.cs 实文件缺失或类名错误。主任务报告了该构建错误；本研究没有重跑它，也没有据此声称插件已修复。来源：[5.6 Build.cs](<C:/Program Files/Epic Games/UE_5.6/Engine/Plugins/Connecter/Source/ConnecterUEPlugin/ConnecterUEPlugin.Build.cs:5>)、[5.6 modules](<C:/Program Files/Epic Games/UE_5.6/Engine/Plugins/Connecter/Binaries/Win64/UnrealEditor.modules>)、[5.7 modules](<C:/Program Files/Epic Games/UE_5.7/Engine/Plugins/Connecter/Binaries/Win64/UnrealEditor.modules>)。

**推断：**独立生成的 HostProject/预览工程可以显式设 `Plugins:[{"Name":"ConnecterUEPlugin","Enabled":false}]`，无需改引擎安装或用户工程。PluginManager 先处理 .uproject 的 Plugins，再添加尚未配置的默认启用插件。另一现成开关是 `.uproject` 的 `DisableEnginePluginsByDefault:true`，然后只显式启用所需插件；这会扩大禁用范围，应由实现按需选择。来源：[PluginManager 的优先处理流程](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Runtime/Projects/Private/PluginManager.cpp:1698>)、[ProjectDescriptor 的开关定义](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Runtime/Projects/Public/ProjectDescriptor.h:104>)。**未验证：**这些设置对本次 BuildPlugin 故障的实际修复结果。

## 3. Asset Registry 索引与依赖

**已核实签名（5.8）：**以下在 `AssetRegistry/IAssetRegistry.h`；5.6/5.7 也有同名现代接口，具体扩展不应靠旧网页例子推断。

```cpp
bool GetAssets(const FARFilter& Filter, TArray<FAssetData>& OutAssetData,
               bool bSkipARFilteredAssets = true) const;
void ScanPathsSynchronous(const TArray<FString>& InPaths,
                          bool bForceRescan = false,
                          bool bIgnoreDenyListScanFilters = false);
void WaitForCompletion();
bool GetDependencies(FName PackageName, TArray<FName>& OutDependencies,
                     UE::AssetRegistry::EDependencyCategory Category =
                         UE::AssetRegistry::EDependencyCategory::Package,
                     const UE::AssetRegistry::FDependencyQuery& Flags = {}) const;
```

来源：[GetAssets](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/AssetRegistry/Public/AssetRegistry/IAssetRegistry.h:363>)、[扫描](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/AssetRegistry/Public/AssetRegistry/IAssetRegistry.h:788>)、[依赖](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/AssetRegistry/Public/AssetRegistry/IAssetRegistry.h:524>)。代码块省略 virtual/纯虚标记，用于展示调用参数。

**已核实：**现代 `FARFilter` 使用 `ClassPaths:TArray<FTopLevelAssetPath>`，还有 PackagePaths、bRecursivePaths、bRecursiveClasses、bIncludeOnlyOnDiskAssets。`ClassNames` 已弃用。索引可用 UStaticMesh、UTexture、UMaterial、UMaterialInstance 的 `StaticClass()->GetClassPathName()`；选 base class 时需决定是否递归包含子类。FAssetData 提供 PackageName、PackagePath、AssetName、AssetClassPath 和 ToSoftObjectPath/GetSoftObjectPath；索引不必 GetAsset。来源：[ARFilter.h](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/CoreUObject/Public/AssetRegistry/ARFilter.h:57>)、[AssetData.h](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/CoreUObject/Public/AssetRegistry/AssetData.h:188>)、[GetSoftObjectPath](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/CoreUObject/Public/AssetRegistry/AssetData.h:345>)、[Epic Asset Registry 说明](https://dev.epicgames.com/documentation/en-us/unreal-engine/asset-registry-in-unreal-engine)。网页中的旧 `Classes`/短类名示例不能直接作为 5.8 代码。

**已核实边界：**GetDependencies 的包接口返回磁盘包已报告的依赖；不会声明“任意动态字符串引用、外部文件、C++ 模块都已收集”。WaitForCompletion 的注释明确：在 OnPostEngineInit 前调用时，尚未加载插件类的资产仍可能未收集，且静默返回。Asset Registry 是引擎子系统，并不是 Rust/JavaScript 的通用裸 uasset 解析器。来源：[依赖接口注释](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/AssetRegistry/Public/AssetRegistry/IAssetRegistry.h:524>)、[WaitForCompletion 注释](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/AssetRegistry/Public/AssetRegistry/IAssetRegistry.h:825>)、[官方概述](https://dev.epicgames.com/documentation/en-us/unreal-engine/asset-registry-in-unreal-engine)。

**推断：**手动索引应在引擎/相关模块就绪后，对已挂载的指定内容路径扫描，保存实际对象路径和来源引擎版本；不要靠文件名猜资产类型或把磁盘绝对路径当作 `/Game/...`。来源：上述扫描、过滤、AssetData 接口。

## 4. UE 生成缩略图与 Kura 最小复用

**已核实：RenderThumbnail 的实际头文件是 `ObjectTools.h`，不是 `ThumbnailTools.h`。** 三个本机版本都有相同参数的声明；5.7/5.8 注释说明传 OutThumbnail 会同步渲染线程。

```cpp
ThumbnailTools::RenderThumbnail(
    UObject* InObject, uint32 InImageWidth, uint32 InImageHeight,
    ThumbnailTools::EThumbnailTextureFlushMode::Type InFlushMode,
    FTextureRenderTargetResource* InRenderTargetResource = nullptr,
    FObjectThumbnail* OutThumbnail = nullptr); // 返回 void

FImageUtils::SaveImageByExtension(
    const TCHAR* Filename, const FImageView& InImage, int32 Quality = 0); // 返回 bool
```

来源：[5.6 ObjectTools.h](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Editor/UnrealEd/Public/ObjectTools.h:755>)、[5.7](<C:/Program Files/Epic Games/UE_5.7/Engine/Source/Editor/UnrealEd/Public/ObjectTools.h:770>)、[5.8](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Editor/UnrealEd/Public/ObjectTools.h:792>)、[ImageUtils.h](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/Engine/Public/ImageUtils.h:109>)。代码块展示参数，非可直接复制的定义。

**已核实：Kura 固定提交 `f8f6d06bf2a626f94cb10b0104b71ce80c6c5458` 的缩略图组件可作为起点。** 本轮通过 GitHub 只读连接重新读取实际源文件，不依赖浮动 main：

- [KuraThumbnailLibrary.cpp](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Private/KuraThumbnailLibrary.cpp)：加载 UObject；调用 RenderThumbnail；等待 asset/shader compilation；收集 UTexture、UMaterialInterface、UStaticMesh 的流送资源并等待；预热缩略图灯光；检查空像素与 PNG 保存结果。UMaterialInterface 分支同时覆盖材质和材质实例。
- [KuraThumbnailLibrary.h](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Private/KuraThumbnailLibrary.h)：UBlueprintFunctionLibrary 中暴露 ExportThumbnailPng、ExportMaterialThumbnailPng；后者临时替换材质 ThumbnailInfo，渲染后恢复原 info 和原 dirty 状态。
- [Kura.Build.cs](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Kura.Build.cs)：原包还含 Slate、ToolWidgets、DesktopPlatform、PythonScriptPlugin 等库 UI 依赖；缩略图使用 UnrealEd/ImageCore。ImageWrapper 在原文件中用于面板 PNG 解码，并非这两份缩略图函数的直接调用。
- [Kura.uplugin](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Kura.uplugin)：1.1.0、Editor/Default 模块、默认不启用；显式依赖 PythonScriptPlugin 和 EditorScriptingUtilities。
- [MIT LICENSE](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/LICENSE)：Copyright 2026 Yukio；复制或实质复用必须保留版权与许可正文。该许可不改变 Epic 引擎的许可。

**已核实的复用限制：**Kura 的 LoadPrimaryAsset 用 `PackageName + "." + GetShortName(PackageName)` 猜主对象路径；本产品已有 AssetData 时可传真实对象路径。Kura 把每个像素 alpha 改为 255，因此输出是用于列表的实心缩略图，不能当保留透明通道的纹理源图。GetUsedTextures 有 5.7 前后条件分支，但源码分支不等于已验证全部引擎版本。来源：[固定 cpp](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Private/KuraThumbnailLibrary.cpp)、[AssetData 真实对象路径](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/CoreUObject/Public/AssetRegistry/AssetData.h:345>)。

**推断：**抽取这两份组件并保留 MIT 通知，使用已有 FImageUtils 输出；不用整套 Kura 库 UI、上传/导入流程或新的 PNG 库。**未验证：**四类具体资产的 ThumbnailRenderer、首帧光照、shader/streaming 等待成本、无窗口渲染、输出质量及全批次成功率。RenderThumbnail 返回 void，桥接层必须把加载失败、空像素、保存失败等结果独立传回。来源：上述 Kura 函数实现与 RenderThumbnail 声明。

## 5. 原生资产编辑器与最小预览工程

**已核实 C++ 入口：**5.6/5.8 的 UnrealEd `Subsystems/AssetEditorSubsystem.h` 都提供以下方法。传 UObject 的重载返回 bool，按路径的重载返回 void；已有窗口会带到前台。

```cpp
bool UAssetEditorSubsystem::OpenEditorForAsset(
    UObject* Asset,
    EToolkitMode::Type ToolkitMode = EToolkitMode::Standalone,
    TSharedPtr<IToolkitHost> OpenedFromLevelEditor = {},
    bool bShowProgressWindow = true,
    EAssetTypeActivationOpenedMethod OpenedMethod =
        EAssetTypeActivationOpenedMethod::Edit);

void OpenEditorForAsset(const FString& AssetPathName,
    EAssetTypeActivationOpenedMethod OpenedMethod =
        EAssetTypeActivationOpenedMethod::Edit);
bool OpenEditorForAssets(const TArray<UObject*>& Assets,
    EAssetTypeActivationOpenedMethod OpenedMethod =
        EAssetTypeActivationOpenedMethod::Edit);
```

来源：[5.6 AssetEditorSubsystem.h](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Editor/UnrealEd/Public/Subsystems/AssetEditorSubsystem.h:117>)、[5.8](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Editor/UnrealEd/Public/Subsystems/AssetEditorSubsystem.h:124>)。这是 UE 内 API，不能把函数名直接当作一个现成命令行参数。

**已核实 Python 最小入口（5.6）：**

```python
asset = unreal.load_asset(object_path)
if asset is None:
    raise RuntimeError("Asset could not be loaded")
opened = unreal.get_editor_subsystem(unreal.AssetEditorSubsystem).open_editor_for_assets([asset])
if not opened:
    raise RuntimeError("Asset editor could not be opened")
unreal.EditorPythonScripting.set_keep_python_script_alive(True)
```

上面的 bool 检查采用官方 `open_editor_for_assets(assets, opened_method=AssetTypeActivationOpenedMethod.EDIT) -> bool`；具体对象路径、工程和错误传回由桥接层提供。来源：[AssetEditorSubsystem Python 5.6](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/AssetEditorSubsystem?application_version=5.6)、[EditorPythonScripting Python 5.6](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/EditorPythonScripting?application_version=5.6)。示例为调用关系说明，本报告未运行。

**已核实命令行及生命周期：**官方完整编辑器入口为 `UnrealEditor-Cmd.exe "Preview.uproject" -ExecutePythonScript="preview.py"`，需要工程启用 PythonScriptPlugin 与 EditorScriptingUtilities。它默认脚本运行后下一 tick 关闭；`set_keep_python_script_alive(True)` 会取消自动关闭。5.6 本机 EditorPythonExecuter.cpp 检查 GWorld/GEngine/GEditor 和 registry 扫描就绪后运行，下一 tick 根据 keep-alive 值决定 QUIT_EDITOR；同一文件从进程命令行解析 `-ExecutePythonScript=`。因此可在独立预览工程内用该现成入口打开内置资产编辑器并保留窗口。`-run=pythonscript` commandlet 不提供原生编辑器 UI；官方也不推荐启动时 ExecCmds 直接跑 py，因为环境可能未就绪。来源：[官方命令行说明](https://dev.epicgames.com/documentation/en-us/unreal-engine/scripting-the-unreal-editor-using-python)、[本机执行器](<C:/Program Files/Epic Games/UE_5.6/Engine/Plugins/Experimental/PythonScriptPlugin/Source/PythonScriptPlugin/Private/EditorUtilities/EditorPythonExecuter.cpp:52>)、[keep-alive 定义](<C:/Program Files/Epic Games/UE_5.6/Engine/Plugins/Experimental/PythonScriptPlugin/Source/PythonScriptPlugin/Private/EditorUtilities/EditorPythonScriptingLibrary.h:21>)。**未验证：**实际有窗口的启动参数组合、聚焦行为、关闭后的宿主清理。

**已核实：**本机有最小 Blueprint 模板 `.uproject`，其 Modules 为空；不需要为每个预览工程新增业务 C++ 模块。来源：[TP_BlankBP.uproject](<C:/Program Files/Epic Games/UE_5.8/Templates/TP_BlankBP/TP_BlankBP.uproject>)。**推断：**生成独立 Preview.uproject，只启用辅助 Editor 工具/所需官方脚本能力，把确认后的资产与依赖迁入其 Content，再调用标准编辑器。模板本身额外启用 ModelingToolsEditorMode 并不是资产预览必需的证据，不应无条件照搬。

**已核实挂载边界：**5.8 `FPackageName::RegisterMountPoint(RootPath, ContentPath)` 返回 mount point 引用；注释说明新挂载位于 override chain 前端，可覆盖旧 mount point。仅挂载外部 Content 不会重新写包内引用，也不是操作系统的只读沙箱。来源：[PackageName.h](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/CoreUObject/Public/Misc/PackageName.h:315>)。**推断：**复用原生迁移并保留包路径，比把任意目录重新挂成 `/Game/` 后假定引用正确更明确；资产编辑器可编辑载入对象，所以保护源素材依赖独立复制/迁移边界，不能只靠“预览”按钮的名字。

## 6. 迁移、依赖确认、返回值

**已核实：5.6/5.7/5.8 公共 C++ 接口共有：**

```cpp
void IAssetTools::MigratePackages(const TArray<FName>& PackageNamesToMigrate) const;
void IAssetTools::MigratePackages(const TArray<FName>& PackageNamesToMigrate,
    const FString& DestinationPath,
    const FMigrationOptions& Options = FMigrationOptions()) const;
```

**已核实：仅本机 5.8 的公共 IAssetTools.h 额外暴露：**

```cpp
void BuildMigrateDependencies(const TArray<FName>& PackageNamesToMigrate,
    TSet<FName>& OutAllDependenciesToMigrate,
    TSet<FName>& OutExcludedDependencies) const;
void MigratePackages(const TArray<FName>& PackageNamesToMigrate,
    const FString& DestinationPath, TSet<FName> AllDependenciesToMigrate,
    TSet<FName> ExcludedDependencies,
    const FMigrationOptions& Options = FMigrationOptions()) const;
```

来源：[5.6 IAssetTools.h](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Developer/AssetTools/Public/IAssetTools.h:566>)、[5.7](<C:/Program Files/Epic Games/UE_5.7/Engine/Source/Developer/AssetTools/Public/IAssetTools.h:626>)、[5.8](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Public/IAssetTools.h:629>)。这些文件在 **Developer/AssetTools/Public**，不是 Editor/AssetTools/Public。5.8 的公开 C++ 依赖集合接口未标 UFUNCTION；不能默认其同名 Python 接口存在。5.8 Python AssetTools 页面本轮不可访问，未作签名保证。

**已核实 FMigrationOptions：**三个版本字段相同。

| C++ 字段 | Python 5.6 名称 | 默认与行为 |
| --- | --- | --- |
| bPrompt | prompt | false；脚本调用总是 false；单参数 C++ MigratePackages 会把它设 true，使用 UE 交互报告 |
| bIgnoreDependencies | ignore_dependencies | false；true 只迁给定包，不适合本次完整依赖迁移 |
| AssetConflict | asset_conflict | EAssetMigrationConflict::Skip；其枚举还有 Overwrite、Cancel |
| OrphanFolder | orphan_folder | 空字符串；头文件注明新迁移实现不用此字段 |

来源：[5.6 字段与构造函数](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Developer/AssetTools/Public/IAssetTools.h:229>)、[5.8 字段](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Public/IAssetTools.h:229>)、[MigrationOptions Python 5.6](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/MigrationOptions?application_version=5.6)、[单参数实现](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Private/AssetTools.cpp:3461>)。

**已核实 Python 5.6 签名：**`unreal.AssetTools.migrate_packages(package_names_to_migrate, destination_path, options=[False, False, AssetMigrationConflict.SKIP, '']) -> None`。package_names 是包名数组，destination 是磁盘目录；`AssetToolsHelpers.get_asset_tools()` 返回工具实例。脚本调用不会自动出现依赖确认 UI。来源：[AssetTools Python 5.6](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/AssetTools?application_version=5.6)、[MigrationOptions](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/MigrationOptions?application_version=5.6)、[AssetToolsHelpers](https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/AssetToolsHelpers?application_version=5.6)。

**已核实迁移实现行为：**

- 5.6 与 5.8 入口先调用 `FEditorFileUtils::SaveDirtyPackages`（包括 map/content），不只是把已选磁盘文件拷出去。bPrompt=false 不是“保证不保存源工程”。来源：[5.6 AssetTools.cpp](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Developer/AssetTools/Private/AssetTools.cpp:3251>)、[5.8](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Private/AssetTools.cpp:3473>)。
- 5.8 BuildMigrateDependencies 遍历 registry 的包依赖，排除 `/Script`、磁盘包不存在项及 CanMigratePackage 不许可项；UWorld 另收集外部对象，首批四类仍可复用该公共实现。来源：[BuildMigrateDependencies](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Private/AssetTools.cpp:4959>)、[RecursiveGetDependencies](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Private/AssetTools.cpp:6027>)。
- 5.8 显式传入确认集合后还会重新计算并 sanitize，不允许传入不在当前许可集合的包；新集合不能视为绕开引擎限制的任意文件清单。来源：[PerformMigratePackages](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Private/AssetTools.cpp:4984>)。
- 官方目的地是目标工程的 **Content** 文件夹；依赖遗漏会使材质等失效。标准交互迁移有 Asset Report，确认后选 Content，冲突可跳过或覆盖。来源：[Epic Migrating Assets](https://dev.epicgames.com/documentation/en-us/unreal-engine/migrating-assets-in-unreal-engine)。
- C++ 返回 void / Python 返回 None，不能据此报告“全部成功”，尤其默认 Skip 时。5.8 GetOnPackageMigration 仅在 AssetTools.UseNewPackageMigration 为 true 时有效；应另核对迁出包、目标冲突与日志结果。来源：[IAssetTools 迁移与事件声明](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Public/IAssetTools.h:629>)。

**推断：**5.8 可直接把 BuildMigrateDependencies 的结果给用户确认；支持 5.6/5.7 时，可展示 registry 依赖闭包，但要标注它不是标准迁移全部排除/特殊路径处理的最终清单，或复用 UE 自己的交互迁移报告。自动脚本应在独立、无脏包的辅助工程上处理副本，显式保持 dependencies=true、冲突默认 Skip，并事后核验。来源：以上版本差异与迁移实现。**未验证：**用户源工程插件、跨引擎版本、bulk data、重定向/插件挂载点与目标工程冲突的实际样本。

## 7. 内容浏览器右键 ToolMenus

**已核实：**5.6/5.8 的内容浏览器基础资产菜单名都是 `ContentBrowser.AssetContextMenu`；引擎按 class hierarchy 从该基础菜单派生。上下文对象是 `UContentBrowserAssetContextMenuContext`，`SelectedAssets:TArray<FAssetData>` 是当前选择。5.5 起 GetSelectedObjects 已弃用，注释明确右键不再自动加载资产；需要实际操作时才载入。来源：[5.6 AssetContextMenu.cpp](<C:/Program Files/Epic Games/UE_5.6/Engine/Source/Editor/ContentBrowser/Private/AssetContextMenu.cpp:195>)、[5.8](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Editor/ContentBrowser/Private/AssetContextMenu.cpp:195>)、[ContentBrowserMenuContexts.h](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Editor/ContentBrowser/Public/ContentBrowserMenuContexts.h:41>)。

**已核实调用顺序与签名：**

| 目的 | API | 主来源 |
| --- | --- | --- |
| 编辑器菜单可用后注册 | `UToolMenus::RegisterStartupCallback(const FSimpleMulticastDelegate::FDelegate&) -> FDelegateHandle` | [ToolMenus.h:145](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenus.h:145>) |
| 扩展现成菜单 | `UToolMenus::Get()->ExtendMenu(FName) -> UToolMenu*` | [ToolMenus.h:169](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenus.h:169>) |
| 放入自有 section | `UToolMenu::FindOrAddSection(FName) -> FToolMenuSection&` | [ToolMenu.h:101](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenu.h:101>) |
| 按选中资产构造 | `FToolMenuSection::AddDynamicEntry(FName, const FNewToolMenuSectionDelegate&)`；该 delegate 参数为 `FToolMenuSection&` | [ToolMenuSection.h:67](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenuSection.h:67>)、[delegate 定义](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenuDelegates.h:22>) |
| 获取选择 | `Section.FindContext<UContentBrowserAssetContextMenuContext>()` 或执行动作的 `Context.FindContext<...>()`，再读 SelectedAssets | [section context](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenuSection.h:91>)、[ToolMenuContext.h:48](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenuContext.h:48>) |
| 菜单动作 | `FToolMenuSection::AddMenuEntry(..., const FToolUIActionChoice& InAction, ...)`；可由 `FToolMenuExecuteAction` 构造，动作参数为 `const FToolMenuContext&` | [AddMenuEntry](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenuSection.h:62>)、[执行 delegate 和 action choice](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenuDelegates.h:29>) |
| 所有权与卸载 | 注册时 `FToolMenuOwnerScoped(this)`；退出时 `UToolMenus::UnRegisterStartupCallback(this/handle)` 与 `UToolMenus::UnregisterOwner(this)` | [owner](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenus.h:531>)、[注销](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenus.h:123>) |

**已核实边界：**RegisterStartupCallback 的注释说明 commandlet、game、server、client-only 不触发。因此右键 UI 和批处理功能应分开入口；不能要求 commandlet 通过菜单注册来执行工作。来源：[ToolMenus.h](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenus.h:140>)。

## 8. 最小依赖与待验证项

以下是依据实际调用的**推断**，不是已构建通过的 Build.cs 清单。只在调用其符号时添加模块；不要复制完整 Kura 依赖。

| 能力 | 最小直接依赖方向 | 依据 |
| --- | --- | --- |
| 通用 UObject/文件处理 | Core、CoreUObject、Engine | Kura 头文件与上述引擎公共类型 |
| PNG 缩略图组件 | UnrealEd、ImageCore；Engine 已覆盖 FImageUtils/编译与流送相关类型 | [固定 Kura cpp](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Source/Kura/Private/KuraThumbnailLibrary.cpp)、[ImageUtils](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/Engine/Public/ImageUtils.h:109>) |
| 索引/依赖与原生迁移 | AssetRegistry、AssetTools | [IAssetRegistry](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/AssetRegistry/Public/AssetRegistry/IAssetRegistry.h>)、[IAssetTools](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/AssetTools/Public/IAssetTools.h>) |
| 右键辅助项 | ToolMenus、ContentBrowser；实际 Slate 菜单动作类型可能需 Slate/SlateCore | [ToolMenuSection](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Developer/ToolMenus/Public/ToolMenuSection.h>)、[ContentBrowser 上下文](<C:/Program Files/Epic Games/UE_5.8/Engine/Source/Editor/ContentBrowser/Public/ContentBrowserMenuContexts.h>) |
| JSON 与薄脚本桥接 | JSON 直接序列化才需 Json；使用官方 Python 启动方案才在工程启用 PythonScriptPlugin/EditorScriptingUtilities | [官方 Python 入口](https://dev.epicgames.com/documentation/en-us/unreal-engine/scripting-the-unreal-editor-using-python)、[Kura.uplugin](https://github.com/Yashashin/Kura/blob/f8f6d06bf2a626f94cb10b0104b71ce80c6c5458/Kura/Kura.uplugin) |

**推断：**当前原生路线无需 GLTFExporter、第三方 GLB/裸 uasset 解析库、完整 Kura UI、另一套图像编码库或网络控制服务。最小组合是现有桌面库界面 + UE 官方 Registry/AssetTools/AssetEditorSubsystem + 固定 Kura 缩略图组件 + 独立预览工程，所有载入/渲染/迁移操作由 UE 做。来源：第 3–7 节已核实 API 和组件。

**未验证，实施时必须取得实际证据：**目标引擎上的 Editor 模块构建；四类代表资产的缩略图与原生编辑器；依赖确认清单与迁移实际结果一致；缺失依赖/同名冲突有明确失败或跳过结果；源工程无意外保存；冷启动、shader/streaming 等待和批次成本；不同 EngineAssociation/第三方类不会被错误当成通用兼容。来源：前述接口不提供这些运行保证；本报告只完成事实核查。
