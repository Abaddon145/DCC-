# 隔离窗口的目录监视

2026-11-02。接在 `485a20a`（Windows CI 前端等待预算，PR #32）之后。`task12bg` 留下的洞是：拒绝写入租约拿不到租约开始后才出现的名字；双读戳也看不出一次已经把修改时间改回去的原地覆写。本切片在租约之后、锚点之前装上父目录监视，一直持有到最后一次窗口核对。

## 现在允许

`open_isolated_readonly` 在产品租约成功之后监视数据库所在目录。监视建立失败就返回「合成课程来源无法只读隔离，请重试」，不复制、不打开 SQLite。

监视的名字只有主库、标记 `dcc-synthetic-course-source.ok`，以及 `-wal`、`-shm`、`-journal`。这些名字上的创建、删除、改名或内容变化都失败闭合，哪怕随后的文件戳和目录戳又与锚点相同。其它名字的事件被读掉，不算命中。队列溢出、监视丢失，或一次读到截断的事件，也失败闭合。

- Linux 使用非阻塞 `inotify`：`IN_CREATE`、`IN_DELETE`、`IN_MODIFY`、`IN_MOVED_FROM`、`IN_MOVED_TO`、`IN_CLOSE_WRITE`、`IN_DELETE_SELF`、`IN_MOVE_SELF`、`IN_ONLYDIR`。不监视 `IN_ACCESS` / `IN_ATTRIB`，避免本门自己的读取（含 atime）被当成活动。
- Windows 使用重叠 `ReadDirectoryChangesW`：文件名、目录名、大小、最后写入时间、创建时间。不监视最后访问时间。比较名字时只折叠 ASCII 大小写。缓冲装不下（`ERROR_NOTIFY_ENUM_DIR` 或返回 0 字节）失败闭合。
- 其它系统这个监视是空操作，和现有的非 Windows 租约一样，不能当作证明。

Linux 测试 `synthetic_course_gate_rejects_inplace_rewrite_when_mtime_is_restored` 在两次读取之间把主库原地改掉再写回原字节，并用 `utimensat` 恢复 mtime。回调里核对过：inode、长度、硬链接数、mtime 与改写前相同，父目录的设备、inode、长度、mtime、ctime 也相同。这些就是双读戳要比较的字段，所以只靠戳会放过。门仍然返回「无法只读隔离」。来源字节不变，没有留下侧文件。回调确实执行过；失败不是发生在改写之前。

## 上一版 Windows 租约测试

`task12bg` 写过五则 `cfg(windows)` 租约测试没有在 Windows 上跑。GitHub Actions `41414453938`（推送 `485a20a`，结论 success）的 Rust 测试日志里这五项都是 ok：

- `windows_shm_deny_write_lease_blocks_share_write_open`
- `windows_database_and_wal_deny_write_lease_blocks_share_write_open`
- `windows_quiet_database_lease_blocks_share_write_without_creating_sidecars`
- `windows_unchanged_database_writer_fails_closed`
- `windows_unchanged_wal_writer_fails_closed`

同一次 `cargo test` 为 819 通过、0 失败、2 忽略。本环境没有重跑它们。这只说明那次 CI 编过并跑过这些测试，不是对正在运行的映流进程所持 SHM 的抢锁。

## 仍然不做

- 监视从租约之后才开始。租约开始前已经完成、并且第一次读取时字节和戳都已经稳定的替换，这次双读和监视都看不到。
- 只改权限或扩展属性（`IN_ATTRIB`、Windows 属性/最后访问时间）不在过滤里。文件戳不含权限位。
- Linux 上 `utimensat` 改不回目录 ctime。创建再删除侧文件之后，即便把目录 mtime 设回去，目录戳的 ctime 已经变了，上一版就会拒绝。本切片不把这个 Linux 形状说成新补上的洞。Windows 目录戳只有创建时间和最后写入时间，`SetFileTime` 可以设回去；`windows_directory_watch_rejects_sidecar_churn_when_directory_times_are_restored` 覆盖「侧文件出现又消失，目录时间被设回」后仍然失败。这则测试没有在本环境编译或运行。
- 本环境没有编译 Windows 目标，因此 `ReadDirectoryChangesW` 路径和上面那则测试都还没有在 Windows 上执行。
- macOS 以及其它非 Linux、非 Windows 系统没有监视。
- 没有制造 inotify 队列溢出或 Windows 通知缓冲溢出。代码路径是失败闭合，没有单独测试。
- 没有读取用户 Vault，没有对正在运行的映流进程抢 SHM，没有安装、NSIS、付费 AI，没有合并 main。
- 本环境没有点击真实系统对话框。前端没有改动，没有跑前端测试。

## 验证

Linux，`rustc 1.89.0 (29483883e 2025-08-04)`，`cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib`。`cargo +1.89.0 fmt --all --manifest-path src-tauri/Cargo.toml -- --check` 与 `git diff --check` 通过。依赖 Windows 目录监视的测试没有编进这次二进制。

| 过滤 | 结果 |
| --- | --- |
| `lease_gate` | 3 通过、0 失败 |
| `marked_course_` | 2 通过、0 失败 |
| `synthetic_course_` | 13 通过、0 失败 |
| `learning_import_runner` | 14 通过、0 失败 |
| `learning_import_generic` | 1 通过、0 失败 |
| `learning::course_` | 49 通过、0 失败 |
| `learning_import_` | 122 通过、0 失败 |

`synthetic_course_` 比上一版多 1 项：`synthetic_course_gate_rejects_inplace_rewrite_when_mtime_is_restored`。
