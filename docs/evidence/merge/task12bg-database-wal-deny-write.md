# 主库与 WAL 的拒绝写入租约

2026-11-02。接在 `54d10a8`（标记闪烁失败闭合，无标记复制退出产品构建，PR #27）之后。上一版只在 Windows 上对已经存在的 `-shm` 使用 `FILE_SHARE_READ`。主库和 `-wal` 仍可以在两次读取之间被其他进程以写入方式打开，只要两端字节和目录戳不变，门就会放过。

## 现在允许

`open_isolated_readonly` 在第一次读字节之前取得产品租约，并一直持有到快照完成、来源再次核对之后。Windows 上这把租约是：

- 主库必须已经存在。先把父目录规范成 `GetFinalPathNameByHandleW` 使用的 `\\?\` 路径，再按原文件名拼接，避免把符号链接解析成另一个文件。然后用已有的 `open_checked_deny_write`（`FILE_SHARE_READ`，拒绝其他写入）持有它。
- `-wal`、`-shm` 仅在已经是普通单链接文件时同样持有。不存在就不创建。
- 目录、重解析点、多余硬链接，或文件已被其他句柄以写入方式打开，租约失败。失败即返回「合成课程来源无法只读隔离，请重试」，不复制、不打开 SQLite。
- 标记检查仍在租约之前。没有标记的数据库不会去打开主库、`-wal` 或 `-shm`。

非 Windows 构建里这个函数是空操作，不创建侧文件。Linux 测试 `non_windows_deny_write_lease_does_not_reject_an_open_writer` 记录了这一点：本进程已经用写入方式打开主库时，隔离仍然成功。这不是 Windows 租约的替代证明。

两次读取之间若有人改写主库，Windows 上该写入会因共享冲突失败，字节保持不变，隔离可以继续。Linux 上没有这把租约，写入会落地，内容摘要不一致，门仍然拒绝。标记或侧文件出现又消失的窗口检查没有撤掉。

## 仍然不做

- 这些 `cfg(windows)` 测试没有在 Windows 上编译或运行：`windows_shm_deny_write_lease_blocks_share_write_open`、`windows_database_and_wal_deny_write_lease_blocks_share_write_open`、`windows_quiet_database_lease_blocks_share_write_without_creating_sidecars`、`windows_unchanged_database_writer_fails_closed`、`windows_unchanged_wal_writer_fails_closed`。它们检查的是本进程先拿到 `FILE_SHARE_READ` 后，再以可写共享方式打开同一主库、`-wal` 或 `-shm` 会失败。不能代替一次在 Windows 上对正在运行的映流进程所持 SHM、主库或 WAL 的抢锁。
- 租约拿不到的名字仍然保护不了。`-wal` 或 `-shm` 在租约之后才被创建，仍只靠目录戳和窗口里的有无来拒绝。
- 选择摘要仍是内容摘要。替换若在租约开始前已经结束，并且 inode 与时间戳被改回锚点，这次双读看不出来。本切片没有做持续目录监视。
- 没有读取用户 Vault，没有安装、NSIS、付费 AI，没有合并 main。
- 本环境没有点击真实系统对话框。

## 验证

Linux，`rustc 1.89.0 (29483883e 2025-08-04)`，`cargo +1.89.0 test --locked --manifest-path src-tauri/Cargo.toml --lib`。`cargo +1.89.0 check --locked --lib`、`cargo +1.89.0 fmt --all --manifest-path src-tauri/Cargo.toml -- --check` 与 `git diff --check` 通过。依赖 Windows 文件租约的测试没有编进这次二进制。

| 过滤 | 结果 |
| --- | --- |
| `lease_gate` | 3 通过、0 失败 |
| `marked_course_` | 2 通过、0 失败 |
| `synthetic_course_` | 12 通过、0 失败 |
| `learning_import_runner` | 14 通过、0 失败 |
| `learning_import_generic` | 1 通过、0 失败 |

`lease_gate` 比上一版多 1 项：`non_windows_deny_write_lease_does_not_reject_an_open_writer`。
