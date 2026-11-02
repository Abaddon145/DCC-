# 合成只读课程来源门

2026-11-02。把选择 → 隔离副本 → 课程预检从仅测试编译的原型里抽出一条可在 Linux CI 上运行的库路径。它只接受带合成标记的 schema 8 样本，不打开真实映流 Vault，也不新增前端路径参数。笔记迁入和外部视频归档未改。

## 现在允许

调用 `select_synthetic_course_source` 再 `preview_synthetic_course_source` 时，来源必须是绝对路径上的普通文件，且同目录有单链接标记文件 `dcc-synthetic-course-source.ok`，内容精确为 `dcc-synthetic-course-source-v1\n`。选择只核对标记、SQLite/WAL 文件头、WAL 与 SHM 是否成对、回滚日志是否缺失，以及主库和侧文件摘要；此步不打开 SQLite。

预检把通过检查的主库（以及成对的 WAL/SHM）复制到临时目录，只在副本上以只读连接打开，再备份到内存并做 schema 8、完整性、外键和课程预检。副本在返回前丢弃。源主库、WAL、SHM 字节保持不变，目标库不新增课程。下面这些情况失败闭合，不返回预检：没有标记、相对路径、符号链接、硬链接数不是 1、存在回滚日志、WAL/SHM 只存在其中一个、文件头无效、两次读取摘要不一致、超时为 0、不是 schema 8、选择被撤销或被新选择替换、来源 epoch 或素材库会话变化。

未检查点的 WAL 帧可以出现在预检里。同一条记录用 `immutable=1` 只读主库时计数为 0，说明预检读到的是副本里的 WAL，而不是把 SQLite 打开在来源路径上。这条路径不是 Tauri 命令，前端不能传入路径。

## 真实 Vault 仍然阻断

- 没有上述标记的文件，包括一份真实映流数据库，在打开 SQLite 之前就被拒绝。CI 只使用仓库里的合成 schema 8 样本和测试现场生成的库，不读取用户 Vault、cookie 或映流数据目录。
- 非 Windows 的课程来源选择、预检和提交命令仍返回仅支持 Windows。本门不提供提交或撤回。
- 既有 Windows 产品选择器仍会复制用户选出的离线库。那是先前 WIP 的路径，不是本门对真实 Vault 的新授权，也不是对正在运行的源进程打开 SQLite。
- 便携门不取得 Windows 的拒绝写入 SHM 租约。活动写入端下的 SHM 保护仍只在 `cfg(test)` 的 `import_snapshot` 原型里，不能当作真实来源授权。
- 标记文件或侧文件名在两次检查之间出现又消失，仍然检测不到。没有做原生桌面验收，没有安装或发布，没有合并 main。

## 验证

Linux 上 Rust 1.89、`cargo test --locked --lib`：

| 过滤 | 结果 |
| --- | --- |
| `synthetic_course_` | 6 通过、0 失败 |
| `learning::course` | 37 通过、0 失败（含上述 6 项） |
| `learning_import_snapshot` | 6 通过、0 失败 |

这是当前 Linux 测试二进制里能编译的子集。依赖 Windows 文件租约的快照、选择和产品迁入测试没有编进这次二进制，本次没有在 Windows 上重跑。`cargo check --locked --lib` 通过，并保留既有未使用代码警告。只被 Windows 调用的 `sidecar` 随后标成 `cfg(windows)`；上表里的课程过滤和快照过滤是在该改动之后重新跑过的。

前端没有改课程选择组件。相关 Vitest：`CourseSqliteImport.test.tsx` 与 `LearningImportPanel.test.tsx` 共 2 个文件、49 项通过。没有重跑前端生产构建。
