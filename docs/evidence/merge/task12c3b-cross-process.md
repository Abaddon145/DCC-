# Task 12c3b：Windows 跨进程文件保护实验

日期：2026-10-28；起点 `0c44dcb`。本阶段只在本机 Windows 临时目录中使用合成数据库和测试子进程。

- 父进程持有测试目录及已存在的 DB/WAL/SHM 句柄；子进程尝试重命名这些对象均被拒绝，随后使用独立 SQLite 连接提交一条数据。父进程的只读快照读取到两条已提交记录。
- 子进程仅接受位于系统临时目录的指定测试路径和随机标记；等待上限 10 秒，超时会结束并回收子进程。测试工具函数单独运行时不操作文件。

## 验证与边界

- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_windows_cross_process -- --test-threads=1`：2 个测试条目通过，退出码 0；其中一个是被父进程再次调用的子进程入口。日志 `release-artifacts/yingliu-merge/task12c3b-target-final.log`。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，540 个文件、0 项疑似凭据。
- 结果只覆盖本机同用户、受控临时目录及已存在侧文件；未验证不存在侧文件注入、完整祖先链、硬链接或重解析点竞争，也未证明 SQLite VFS 打开的对象身份和单独持有的句柄一致。源程序 checkpoint、关闭、移动及长期写入的影响仍未验证。
- 本阶段只新增测试，生产代码和前端未变化；未读取实际映流资料、迁入用户数据、安装或发布。上一步完整 Rust 回归为 591 通过、2 项既有忽略，前端构建通过；此处未重复全量回归或构建。
