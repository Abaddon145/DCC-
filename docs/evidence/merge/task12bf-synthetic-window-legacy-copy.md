# 合成来源双读窗口与无标记复制退出产品构建

2026-11-02。接在 `71c52bd`（Windows 课程选择接到合成标记门，PR #26）之后。那一版的两次读取只比较内容摘要。标记或侧文件在两次检查之间出现又消失时，两端字节可以一样，门会放过。无标记复制辅助仍编进 Windows 产品构建。本切片补上双读窗口的失败闭合，并把这些辅助从产品构建里拿掉。

## 现在允许

`open_isolated_readonly` 在第一次读字节之前记下窗口：父目录戳、标记文件身份、主库身份，以及成对 `-wal` / `-shm` 的有无和身份。回滚日志必须不存在。两次内容摘要必须相同，窗口也必须相同。复制到临时目录、做内存快照之后再比一次。不一致就返回「合成课程来源无法只读隔离，请重试」，不把这次连接交出去。

Linux 测试覆盖两种原先漏掉的形状，来源主库字节不变，来源旁不留下 WAL/SHM：

- 两次读取之间写入再删除 `-wal`、`-shm`、`-journal`，并删掉标记再写回相同正文。
- 标记文件不换 inode，只原地覆写相同正文。修改时间变了，窗口不一致。

安静的带标记样本仍能读到 schema 8。文件身份不含 atime/ctime，避免读文件本身改时间戳造成误拒。目录戳在 Unix 上含设备、inode、长度、mtime 和 ctime。Windows 上目录戳是创建时间和最后写入时间，文件戳是属性、创建时间和最后写入时间。这条 Windows 比较没有在本环境执行。

无标记复制不再进入产品构建：

- `select_sqlite_with_picker`、`verify_sqlite_selection`、`release_sqlite_selection`、`current_sqlite_selection` 只在 `cfg(test)` 下编译。
- `course_snapshot::open` 和 `LockedSnapshot` 只在 `cfg(all(test, windows))` 下编译。
- `course_import_legacy`（含 `preview_selected`、`commit_selected`）只在 `cfg(all(test, windows))` 下编译。
- `source_sqlite` 整模块改为 `cfg(all(test, windows))`。产品构建里不再有 `open_candidate`。

课程界面命令和 `commitCourse` worker 仍只走带标记的合成会话。Windows 上 `sqlite_import_selection` 互斥量还在，产品构建里它是空的，不保存路径或批准。

## 仍然不做

- 主库和 `-wal` 没有拒绝写入租约。窗口戳能拒绝两次检查之间出现又消失的名字，不能拒绝一个字节和目录戳都不变、但正被其他进程写入的主库。
- 选择时留下的摘要仍是内容摘要。替换若在两次读取开始前已经结束，且正文与文件身份在窗口内保持不动，这次双读看不出来。本切片没有做持续目录监视。有人若在第二次观察前把目录时间戳和文件身份改回锚点（例如 `utimensat`，并且 inode 被复用），比较也会通过。
- `windows_shm_deny_write_lease_blocks_share_write_open` 仍只在 `cfg(windows)` 下编译。Linux CI 不能代替一次在 Windows 上对正在运行的映流进程所持 `-shm` 的抢锁。
- Windows 文件租约回归（`course_import_tests`、`import_access_tests`）没有在 Windows 上重跑。NTFS 上目录最后写入时间是否会因创建或删除侧文件而变化，本环境没有证明。
- 没有读取用户 Vault，没有安装、NSIS、付费 AI，没有合并 main。
- 本环境没有点击真实系统对话框。

## 验证

Linux，`rustc 1.89.0 (29483883e 2025-08-04)`，`cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib`。`cargo +1.89.0 check --locked --lib`、`cargo +1.89.0 fmt --all --manifest-path src-tauri/Cargo.toml -- --check` 与 `git diff --check` 通过。依赖 Windows 文件租约的测试没有编进这次二进制。

| 过滤 | 结果 |
| --- | --- |
| `lease_gate` | 2 通过、0 失败 |
| `marked_course_` | 2 通过、0 失败 |
| `synthetic_course_` | 12 通过、0 失败 |
| `learning_import_runner` | 14 通过、0 失败 |
| `learning_import_generic` | 1 通过、0 失败 |

`synthetic_course_` 比上一版多 1 项：`synthetic_course_gate_rejects_marker_or_sidecar_that_appears_and_disappears`。
