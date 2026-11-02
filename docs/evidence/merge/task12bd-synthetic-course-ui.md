# 非 Windows 课程界面接到合成预检

2026-11-02。接在 `f4eb8cc`（合成预检批准与 `commitCourse`）之后。课程选择、预检、排队已经在界面上，但非 Windows 的 Tauri 命令仍直接返回仅支持 Windows，界面走不到刚落地的批准。本切片把这些命令接到带标记的合成会话。

## 现在允许

非 Windows 上，课程来源选择、当前选择、撤销、系列列表、预检和排队使用原有命令名。选择只打开系统文件对话框，命令参数里没有路径。对话框返回的路径先检查同目录标记 `dcc-synthetic-course-source.ok` / `dcc-synthetic-course-source-v1\n`。没有标记的数据库，包括放在另一目录的 schema 8 样本，在打开 SQLite 之前被拒绝，也不会写出 WAL/SHM。

预检命令调用 `approve_marked_course_preflight`。它复制标记来源，只在副本上预检，并把约 15 分钟、一次性、不落库的批准留在本进程。批准绑定当前 scope、profile、epoch、selection、来源字节摘要和课程源/目标指纹。可选的系列列表沿用既有分批预检；不存在的系列不会批准整库。再预检一次会换掉旧批准，所以修改媒体绑定或系列范围后，旧的预检 UUID 不能排队。预检不写课程、批次或媒体文件。媒体仍只能是当前库已有视频的 UUID。

排队命令只接受选择 UUID 和预检 UUID，并排入现有 `commitCourse`。通用 IPC 和通用重试仍然拒绝这条任务。Worker、回执和既有 `revert_course_batch` 没有改。任务中心回执和批次在终态刷新仍由现有 `commitCourse` / `cancelCourse` / `revertCourse` 结果驱动。界面上的撤回按钮也没有改。

课程区文案改为：页面不能提交路径；非 Windows 只接受带合成标记的只读样本。Markdown 目录绑定和笔记迁入命令仍返回仅支持 Windows。

## 仍然不做

- 没有 Windows 拒绝写入 SHM 租约。活动写入端下的 SHM 保护仍只在 `cfg(test)` 的 `import_snapshot` 里。Linux CI 不能代替一次在 Windows 上对稳定占用文件的写句柄抢锁。
- 标记或侧文件在两次检查之间出现又消失，仍然检测不到。
- Windows 产品选择器没有改，仍会复制用户选出的离线库。那是先前的离线路径，不是本切片对真实 Vault 的新授权，也不是对正在运行的源进程打开 SQLite。
- 没有前端路径参数，没有读取用户 Vault，没有安装、发布或合并 main。
- 本环境没有点击真实系统对话框。测试注入的是对话框回调返回的路径，命令签名本身不接收路径。

## 验证

Linux，`rustc 1.89.0`，`cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib`。`cargo +1.89.0 fmt --all --manifest-path src-tauri/Cargo.toml -- --check` 与 `git diff --check` 通过。

| 过滤 | 结果 |
| --- | --- |
| `marked_course_` | 2 通过、0 失败 |
| `synthetic_course_` | 11 通过、0 失败 |
| `learning_import_runner` | 14 通过、0 失败 |
| `learning_import_generic` | 1 通过、0 失败 |

Vitest：`CourseSqliteImport.test.tsx`、`LearningImportPanel.test.tsx`、`TaskCenter.test.tsx` 共 3 个文件、100 项通过。没有在 Windows 上重跑依赖文件租约的产品迁入测试。
