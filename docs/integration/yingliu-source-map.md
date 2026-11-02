# 映流 → 栈藏来源冻结与逐文件适配图（M0）

日期：2026-10-21。本文件只冻结准备材料，不复制模块、不执行产品迁移。

2026-10-22补充：已获目标产品顺序实施授权；源端94ee1e8提供只读迁入输入v1。下列新文件hash已按实际字节核对，与源SHA清单全部一致：合约 `4e5f3c2d20ce5633a308f1a23d4ffcf5bd4a0a1743ea38547c661ae9a9d4cf69`，机器样例 `5618b1423558f3b40759886d93dde207802741458cfec305117d076cbf1a98db`，checks.json `84e28eaa60cb47e6a84062137ed67f699bc6d0b690c1a3e243f5f6fb520db88d`，source-tools.json `c8baea4551a11a47ce6c6521a3bcf7e213e39e29de82b284ae70246565ad2380`，输入QA脚本 `9e461242f2e2a04028a0c634d16918d5420a33bec55b84efca99298863eb3884`，工具QA脚本 `ed520e9a95b50e423e47527698ee57840d68fcef91b5c12c02ca0981c6e273b9`。源代码修复仍固定c866508；本端只读文件，没有运行源服务或读取真实源库。M6格式已提供，待实现目标验证/映射，而非等待对方提供格式。

## 精确基线与证据

| 对象 | 固定值 |
| --- | --- |
| 目标准备起点 | `2f8b75f`；实际产品仍为 v0.23.0 / schema 22，产品代码基线48078e5，测试增量fcddd01 |
| 源主工作区 | `C:/Users/14582/Documents/ChatGPT/视频下载软件` |
| 源最新文档 HEAD | `0eef17b9bddcbb51677d220fa0589a01e7755c93` |
| 已提交分析修复/移植代码冻结 | `c86650831aecb55ef7bec288df9ae7fb27c8d178` |
| 源完整旧产品起点 | `67ba053126a0df6bbd1c5d7ce9aa22290f436480`，v0.5.2 |
| 源 readiness SHA256 | `9D7DB79ABEB8F4C6E6A8D226597FE3BE21FDE16793BC2C5CC08A7C281A7C14A3` |
| source-analysis-fix.md SHA256 | `09FED5150ECF92C800F0D986C17B7377DBA3F8597CBB99802B5B66FACBC6A1D8` |
| section-reference-fixture.json SHA256 | `FADDCF711C5A36A37E62E12EB97D0A96409EAB34D039FF6C56F4CA9723241D75` |

实际只读Git检查：c866508是0eef17b祖先，二者在src、src-tauri及源分析浏览器检查脚本上的diff为空。因此源端分析修复**已可按提交核验，不再列为缺失提交阻断**。源端当时仅剩协调设计未跟踪，不据此迁入未提交产品代码。

源报告的101前端、266 Rust通过/8忽略、构建成功和4组浏览器模拟检查已阅读；result.json明确mocked=true、realDesktop=false、paidApiCalls=0。没有在DCC重跑源全量，也未把这些数字称为DCC集成通过。真实用户错误回执与真实原生/DPI仍不在这份证据中。

## 逐文件来源 → 目标

左侧均相对映流，右侧均相对栈藏；目标“新”表示计划位置，不是已经移入。37个选定源文件本轮实际存在并计算SHA256，完整清单见 [source-files.json](../evidence/merge/source-files.json)。原路径/接口变化后重新固定，不整分支cherry-pick。

| 源文件 | 目标落点 / 处理 |
| --- | --- |
| src-tauri/src/learning/native_player.rs | 新learning/native_player.rs；仅移会话/IPC/几何，去掉源LearningService/DB耦合 |
| src-tauri/src/learning/native_window.rs | 新learning/native_window.rs；适配DCC主窗口/关闭屏障/DPI |
| src-tauri/src/learning/progress.rs | 新learning/store.rs；毫秒/sequence算法适配目标epoch与revision |
| src-tauri/src/learning/player.rs | 新learning/subtitles.rs及media_access.rs；不保留源任意path播放接口 |
| src-tauri/src/learning/models.rs | 新learning/models.rs；择取课程/进度/轨道DTO，改关联目标media ID |
| src-tauri/src/learning/repository.rs | 新learning/store.rs；源读取只作M6快照字段参考，不在原库Repository::open |
| src-tauri/src/learning/capture.rs | 新learning/excerpts.rs；调用目标knowledge_files/reference_boards，不移Vault写入器 |
| src-tauri/src/learning/cache.rs | 新learning/files.rs；复用目标file_digest和原子文件函数 |
| src-tauri/src/learning/commands.rs | 新learning/commands.rs；重写为库ID/课程ID命令，不照搬命令注册 |
| src-tauri/src/learning/service.rs | AppState/learning/commands.rs；只移必要编排，不迁第二套App/连接/设置 |
| src-tauri/src/learning/db.rs | M6只读快照/字段参考；目标仍用db::open_database，禁止执行源迁移 |
| src-tauri/src/native.rs | runtime/process.rs及新native_window的必要Win32部分；不搬源凭据/输入窗/全局状态 |
| src-tauri/src/ai/analysis.rs | 新learning/analysis.rs；移来源真实性校验/局部恢复，保留严格关卡 |
| src-tauri/src/ai/models.rs | 新learning/models.rs；课程结果与资产摘要区分，保留可信报告类型 |
| src-tauri/src/ai/references.rs | 新learning/analysis.rs；来源ID/毫秒/不连续范围验证 |
| src-tauri/src/ai/repository.rs | 新learning/analysis_store.rs；接入当前库和cache来源，不能用源服务recover |
| src-tauri/src/ai/summary.rs | 新learning/analysis.rs；汇总保留合法分段与程序校验说明 |
| src-tauri/src/ai/jobs.rs | tasks.rs + 新learning/analysis.rs；只增任务类型，不保留源队列 |
| src-tauri/src/ai/section_reference_tests.rs | 新learning/analysis_tests.rs；移最小fixture/断言并经目标interface重跑 |
| src-tauri/src/ai/template_tests.rs | 新learning/analysis_tests.rs；择取模板/汇总/重复来源回归 |
| src-tauri/src/ai/materials.rs | 新learning/analysis.rs的材料准备部分，复用目标文件解析/指纹 |
| src-tauri/src/ai/subtitles.rs | 新learning/subtitles.rs；稳定cue和完整文本，不移截断行为 |
| src-tauri/src/ai/transcription.rs | M3新learning/transcription.rs（子计划细化）；复用目标进程/FFmpeg与任务 |
| src-tauri/src/ai/whisper_models.rs | M3工具/模型接入参考（子计划细化）；不默认下载模型 |
| src-tauri/src/ai/qa.rs | 课程结果来源/预算参考；问答首批范围在M3子计划明确，不直接搬整个服务 |
| src-tauri/src/ai/service.rs | 新learning/analysis.rs编排参考；复用目标TranslationService/任务，不迁第二套凭据和recover |
| src/features/learning/LearningPlayer.tsx | 新LearningWorkspace.tsx组合行为，使用目标导航/保存屏障 |
| src/features/learning/NativeLearningSurface.tsx | 新NativeLearningSurface.tsx；适配当前窗口与事件版本 |
| src/features/learning/nativeBindings.ts | 新nativeBindings.ts；F11/默认全键修正后迁入，不能照搬小写bug |
| src/features/learning/playerInteraction.ts | 新playerInteraction.ts；补单/双击、视频区域滚轮测试 |
| src/features/learning/playbackHandle.ts | 新学习会话清理/RequestGate；不迁源App钩子 |
| src/features/learning/VideoNotes.tsx | 当前KnowledgeWorkspace + 新摘录预览；不保留第二份正文编辑器 |
| src/features/ai/VideoSummary.tsx | M3新CourseSummary（子计划具体化），课程结果页签 |
| src/features/ai/SectionValidation.tsx | M3课程校验说明组件；纯文本安全渲染 |
| src/features/ai/Transcript.tsx | 新TranscriptPanel.tsx；删除静默slice截断，按页完整取cue |
| src/features/ai/VideoAsk.tsx | 限长/预填问题参考，首批不直接搬其二次截断及问答框架 |
| src/types/ai.ts | 新learning/types.ts；只选课程DTO及可信校验说明，不替换目标全局types |

后续模板/字幕格式/导出协议另由M3/M6子计划固定。本表并不声称37个文件均需要整份移入。

## 工具实际位置、校验与兼容选择

根目录简称：

- **D** = `C:/Users/14582/Documents/ChatGPT/软件开发/src-tauri/resources/media-tools`
- **S** = `C:/Users/14582/Documents/ChatGPT/视频下载软件/src-tauri/resources/tools`
- **W** = `C:/Users/14582/.codex/worktrees/content-updates/视频下载软件/src-tauri/resources/tools`

S当前仅有yt-dlp、Deno和独立FFmpeg/FFprobe，没有mpv/Whisper。W实际完整，18个manifest登记exe/DLL全部存在，哈希逐项相符；D的4个exe及7个shared DLL已计算哈希，mpv/Whisper仍未复制进去。

| 文件 | 实际位置 | SHA256 / 状态 | 许可/选择 |
| --- | --- | --- | --- |
| ffmpeg.exe | D | `08dd3b1c2e9b8c9b2489ac2e1265bb21fb57941b930b1a82adcc5d662784472b` | n9.0.2-20260919；保留目标shared LGPL构建 |
| ffprobe.exe | D | `1b61d47afd7f629bece8035f4f5b44c8d3ee937dda63e334e8f8a1205550d2f5` | 与目标FFmpeg同套 |
| yt-dlp.exe | D/S/W | `66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a` | 2026.08.19；独立发行组合GPLv3+，不重复搬 |
| deno.exe | D/S/W | `e020f3e232bd16e33768dee528e5983349c962952051ced0a5d58ad42f5d9b33` | 2.9.7 MIT，不重复搬 |
| ffmpeg.exe | S/W | `02b611a1ce617a68ca1742fb848fe41c728f092996ff2e4820d401554d2d2453` | 源GPL构建，本轮只生成fixture，不替换D |
| ffprobe.exe | S/W | `8e92d4612c4199f26186819f49ebdd585c46c988da915296df9d18e8caeabc04` | 源GPL构建，不复制 |
| mpv.exe | W | `9114ed8100da6b89c5053486392e6c69a8ef6917704a2837b2b81b1f4bd03000` | 20261015-git-492bae0768，GPL-2.0-or-later及其依赖许可；--no-config --version通过 |
| whisper-cli.exe | W | `58245314fb73b30fbd0cf0542c5c172e23f02b6eb7cad7b51e792439cf5e1755` | 1.9.1 MIT；--help通过，未下载模型/转录 |
| mpv/fonts.conf | W | `f141c1b89b172d22f213531646c21e288f0ebf3ec46484698896e1b33c626756` | 存在，正式迁入保留相对布局 |

源mpv/Whisper固定归档在W上实际存在并复算匹配components.json：mpv 7z=`f9becefe40e2a29d2ce5f3ab231ff094f9cf3e4038ff014cfe4a0c09ad90e79e`，Whisper zip=`7d8be46ecd31828e1eb7a2ecdd0d6b314feafd82163038ab6092594b0a063539`。exe/DLL匹配现有binaries.json不等于本轮重新解压重建；没有下载或拷贝这些工具。

W上mpv-Copyright/GPL-2/LGPL-2.1、Whisper MIT和GPL-3文本存在且已hash；完整文件/DLL/许可哈希在M0结果JSON。mpv自身报告内部FFmpeg为N-127552-gee8cbd7db，和源外置FFmpeg、DCC外置FFmpeg各自独立，不能由其内部版本推导必须替换目标工具。

**兼容选择：保留DCC外置FFmpeg/FFprobe。** 实际buildconf为enable-shared、enable-version3、未enable-gpl，随包有LGPLv3文本；实际测试能完成H.264/HEVC解码、双音轨探测、截图、VP9+Opus代理、PCM16/16kHz/单声道提取和波形，以及AVI/MPEG4、WMV2解码。GPL源FFmpeg未进入DCC。此结论不证明所有真实用户视频兼容，也不证明mpv已集成；后续出现具体不支持格式再逐项评估构建。

## 待补依赖与不迁入项

1. M3不再缺已提交修复；仍需目标课程结果/预算适配子计划，以及来源/模板/缓存版本规则的目标fixture。
2. M2工具实体/许可已可核查；实际拷入、目标资源配置、进程/IPC、DPI、焦点和完整键位仍等待产品集成授权后验证。M0的--version/--help不代替真实播放。
3. M6版本化只读输入已由94ee1e8提供并核对6项摘要；待目标验证器、source-profile/笔记vault_id+path映射、sidecar与归档发布实现及验收。现有源服务初始化会迁移/恢复任务，仍禁止用它读取真实用户原库；合成manifest通过不代表目标迁入器已实现。
4. 不迁源App、数据库入口、全局settings、旧任务队列、凭据/DPAPI Key/Cookie、运行态付费授权、整个工具目录、整个Vault写入系统及双应用协议实现。

Task2开工前检查清单与样本范围见 [M0证据](../evidence/merge/m0.md)。本端准备不授权修改schema或迁移用户库。
