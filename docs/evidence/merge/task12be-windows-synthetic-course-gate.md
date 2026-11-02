# Windows 课程选择接到合成标记门

2026-11-02。接在 `b524c37`（非 Windows 课程界面接到合成预检）之后。Windows 产品命令原先走离线选择：对话框返回的数据库可以没有标记，预检和 `commitCourse` worker 会复制该文件再打开副本。本切片把 Windows 的课程选择、当前选择、撤销、系列列表、预检和排队接到同一条带标记的合成会话。

## 现在允许

Windows 与非 Windows 使用同一组命令，实现都在 `course_import`。选择只打开系统文件对话框，命令参数里没有路径。对话框标题是「选择带合成标记的课程数据库副本」。返回的路径先检查同目录标记 `dcc-synthetic-course-source.ok`，内容精确为 `dcc-synthetic-course-source-v1\n`。没有标记的数据库，包括另一目录里的 schema 8 样本，在打开 SQLite 之前被拒绝，也不会为它写出 WAL/SHM。未标记路径不会去打开 `-shm`。

预检调用 `approve_marked_course_preflight`。它复制标记来源，只在副本上做 schema 8 预检，并把约 15 分钟、一次性、不落库的批准留在本进程。批准绑定当前 scope、profile、epoch、selection、来源字节摘要和课程源/目标指纹。排队只接受选择 UUID 和预检 UUID，排入现有 `commitCourse`。Windows 与非 Windows 的 worker 都执行 `commit_synthetic_course`。通用 IPC 仍然拒绝这条任务。笔记迁入和 Markdown 目录绑定没有改，非 Windows 上仍返回仅支持 Windows。

标记来源若已经有 `-shm`，Windows 在打开 SQLite 之前用 `FILE_SHARE_READ` 持有该文件（拒绝其他写入）。抢不到租约就返回，不打开 SQLite。没有 `-shm` 时不创建它。非 Windows 构建里这一步是空操作。Linux 上用注入的失败租约证明：拒绝发生在复制和打开 SQLite 之前，来源旁不出现 WAL/SHM。

## 仍然不做

- Linux CI 没有运行 `windows_shm_deny_write_lease_blocks_share_write_open`。该测试只在 `cfg(windows)` 下编译，检查的是本进程先拿到租约后，再以可写共享方式打开同一 `-shm` 会失败，同时普通读取仍然成功。它不能代替一次在 Windows 上对正在运行的映流进程所持 SHM 的抢锁。
- 主库和 `-wal` 没有拒绝写入租约。两次读取之间的字节变化会失败闭合。一个在两次读取之间字节不变、但正被其他进程占用的主库，不能靠这把 SHM 租约拒绝。
- 标记或侧文件在两次检查之间出现又消失，仍然检测不到。
- `select_sqlite_with_picker`、`course_snapshot::open`、`preview_selected` 和 `commit_selected` 仍留在 Windows 测试里，供既有文件租约回归使用。课程界面命令和 `commitCourse` worker 不再调用它们。这些辅助函数仍然可以复制测试选出的无标记离线库；那不是产品命令，也没有前端路径参数。
- 没有读取用户 Vault，没有安装、NSIS、付费 AI，没有合并 main。
- 本环境没有点击真实系统对话框。测试注入的是对话框回调返回的路径。

## 验证

Linux，`rustc 1.89.0`，`cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib`。`cargo +1.89.0 fmt --all --manifest-path src-tauri/Cargo.toml -- --check` 与 `git diff --check` 通过。依赖 Windows 文件租约的产品迁入测试没有编进这次二进制，也没有在 Windows 上重跑。

| 过滤 | 结果 |
| --- | --- |
| `lease_gate` | 2 通过、0 失败 |
| `marked_course_` | 2 通过、0 失败 |
| `synthetic_course_` | 11 通过、0 失败 |
| `learning_import_runner` | 14 通过、0 失败 |
| `learning_import_generic` | 1 通过、0 失败 |

Vitest：`CourseSqliteImport.test.tsx`、`LearningImportPanel.test.tsx`、`TaskCenter.test.tsx` 共 3 个文件、100 项通过。
