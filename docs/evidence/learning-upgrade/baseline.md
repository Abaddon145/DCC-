# 栈藏学习联动 Phase 0：本端基线与恢复证据

日期：2026-10-21（北京时间）。只在栈藏仓库操作，映流代码/原设计/原计划/协议均只读。

## 基线

| 项目 | 实际结果 |
| --- | --- |
| 起点 HEAD | `48078e56e0b304dc313a03c9aba4ef010847dacb` |
| 原分支 / 新分支 | `codex/v0.23-asset-intelligence` → `codex/yingliu-learning-integration` |
| 初始工作区 | 干净；未发现本仓库 AGENTS.md |
| 应用版本 | package.json、Cargo.toml、tauri.conf.json 均为 0.23.0 |
| 当前 schema | 22；v0.23 保留资产核心、关系、分析缓存、Prompt、Shot |
| 原方案 PR #14/v0.21 | 仅历史参考，不检出/回退/覆盖其后的本地成果 |
| 安装状态 | 本轮未启动/安装/覆盖应用；当前用户卸载注册表未发现匹配项，不能据此断言未安装。未确认用户正在使用的安装版本。 |
| 远程状态 | 本阶段不联网更新分支、不推送、不合并、不发布 |

采用当前会话顺序执行，使用新本地分支隔离提交。没有启动实施子代理，也没有修改另一窗口工作区。

## 本轮命令与结果

日志在本仓库忽略目录 `release-artifacts/yingliu-learning-integration/phase0/`。没有用户字幕、回执正文、密钥或私有媒体。

| 命令 | 结果 / 日志 |
| --- | --- |
| `npm test` | 正常 Windows 权限：64 文件、315 项通过，0 失败；`frontend-normal-permissions.txt` |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1 --nocapture` | 331 通过、0 失败、2 既有忽略；main/doc 各 0；`rust-final.txt` |
| `npm run build` | TypeScript + Vite 构建成功；`build.txt` |
| 新增双库验收 | `backup::tests::learning_baseline_two_libraries_preserve_scoped_identities_and_media_hash` 通过，纳入 Rust 全量 |

初始未改代码的 Rust 基线为 330 通过、2 忽略（`rust.txt`）。前端首次在沙箱内运行时，63 个文件在加载阶段因沙箱 Temp 下模块缓存 ENOENT 失败，14 项通过（`frontend.txt`）；正常权限原样重跑 315 通过。未修改配置/删除测试规避该环境错误。

新增验收首次编译暴露测试代码读取 `CreativeProject.id` 的笔误，修正为实际模型 `CreativeProject.summary.id` 后通过；这是测试适配修正，不是产品功能的 RED→GREEN 声明。本阶段未增加产品行为。

两个既有忽略项：`knowledge::tests::benchmark_twenty_thousand_notes_and_project_pages`（大数据手工基准）、`note_remote_images::tests::real_markdown_images_are_managed_offline`（需真实网络样本）。没有运行付费 AI 或真实站点。

构建保留现有 >500kB 分包警告，主包约 1497.11kB、知识库包约 611.15kB；Rust 保留既有 dead-code/MSVC 链接器警告。本阶段未生成 NSIS 包。

### 日志 SHA256

- frontend-normal-permissions.txt：`058AE8F0CE75943DF73DAD9E9AFDEB99D4CF2EE5FC2CA3B7BBF6E84560257B2D`
- rust-final.txt：`AE90FEFD02520A5E4E2D2CF2075B97DC747565D6F969FF7B9105FE9DCB060F24`
- build.txt：`50A38C7A062CD5ED897E3EC47098D0400511BADB79FBD60DC5FD88FA35D1D141`

## 双隔离库备份与恢复

新增检查在两个 `tempfile::tempdir()` 中创建独立数据库和不同 UUID 的 library.json；来源生成 8×6 PNG，不读用户文件。源媒体样本 135 字节，SHA256 `e76aab9983ae9d6d25064ad7a2bd0a91a1594f8e99bf55bc09a5aec9fcb63205`。

实际调用现有媒体导入、项目关联、笔记保存、附件导入、参考板快照和 `backup::export_library` / `restore_library`。验证：

- 恢复前目标库无法解析源媒体；恢复后媒体、核心资产、登记文件 UUID 保留且映射相同。
- 目标 library.json 及其库 UUID 保持原样；源 library.json 保持不变。**跨库恢复不是源库身份迁移**。
- 托管原件、笔记附件均与源 SHA256 相同；源 PNG 和原笔记正文未变。
- 笔记文档及媒体/项目引用保留且可用；项目媒体关系、参考板图片数保留。
- schema 22、foreign_key_check 为 0；恢复前安全备份沿用已有流程。

另有全量既有检查覆盖 v0.23 分析/关系/Shot/Prompt 回收恢复、笔记历史与附件、损坏备份、中断恢复、任务安全屏障等。本轮未备份/恢复真实用户活动库。PNG 是数据库/文件恢复样本，不是视频播放或真实双应用联调证据。

## 实体身份与复用点

| 领域 | 已核实代码 / 约束 |
| --- | --- |
| 素材库 | `state::LibraryMarker.library_id`；`with_library_if_id` / `with_library_mut_if_id` 可阻止迟到请求跨库访问；复制/恢复场景需显式区分身份。 |
| 核心资产 | `asset_core.id` 对应 legacy/media/entity origin，`asset::repository::origin` 转换；不能与 `assets.id`、`media_entries.id`、`asset_media.id` 混用。 |
| 原件 | `media_entries.primary_file_id`、`media_files`、`asset::repository::file_path`；读取登记原件且遵守 `media::safe_relative`，不用代理冒充原件。 |
| 项目 | `projects.id`、归档状态、`project_media_entries`、现有 project_units/Shot；复用项目工作区。 |
| 笔记 | `knowledge_notes.id`、expected_revision、`knowledge_references`、附件 UUID；本地引用限定 note/asset/media/project，没有外部映流类型。 |
| 参考板 | `reference_boards` 自有快照、`project_reference_boards`；新增摘录图需走安全复制，不能指向临时交换图。 |
| 任务与维护 | `tasks`、备份维护屏障、切库保护、关闭知识编辑保存屏障；联动不能绕过。 |

## 启动与准确导航

- `main.rs` 只分流浏览器宿主注册与 native-host 参数；目前没有 `--handoff-request`。
- `lib.rs::run` 首先持有 `browser_capture::application_lock`。第二实例通过 `request_open` 写 open.json 后退出；已运行实例的 3 秒 pump 读取后唤起窗口，仅有 time，没有实体身份。
- 复用现有单实例生命周期；后续协议接收需有独立的持久请求/回执通路，不能把唤醒成功当作实体打开或业务完成。
- 前端已有 `MediaLibraryView.initialEntryId`、`ProjectHub.initialProjectId`、`KnowledgeWorkspace.initialNoteId`，适合接入准确导航；须核验当前库、目标存在性、归档/回收状态和未保存草稿。
- Tauri 标识 `com.local.dccassetlibrary`；配置未注册系统 deep-link 插件/任意路径打开接口。外部启动方式待协议共同确认后实现。

## 后续门槛与未验证范围

协议开始检查时不存在，准备期间映流窗口提供了提议版。审阅见 `docs/integration/yingliu-dcc-v1-review.md`；待双方确认一致文档/样例 SHA256。Phase 5—6 尚未开始。

本报告证明本端当前代码、生成测试库的恢复和生产构建；没有证明真实双应用冷/热启动、截图/字幕传输、安装配对、实际用户数据恢复、性能 p95 或跨屏 DPI。映流分析故障/编辑样本不在本窗口职责内。协议确认前不新增另一套协议或产品联动入口。
