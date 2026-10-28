# Task 12c3a：Windows 活动 SQLite 文件保护实验

日期：2026-10-28；起点 `0a9471f`。本阶段仅在临时合成目录中增加 Windows 测试，没有产品来源读取入口。

- 对测试目录及已存在的 DB、WAL、SHM 打开允许读写但不共享删除的句柄后，同进程中对这些对象的重命名均被拒绝。SQLite 写连接保持打开，继续提交一笔数据；测试用只读快照读取到两笔已提交数据。
- 单独的负例验证：DB 和目录被持有时，原先不存在的 WAL 文件仍可被创建。因此保护已有句柄不能自动保护不存在的侧文件。

## 验证与边界

- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_windows_lease -- --test-threads=1`：2 通过，退出码 0。日志 `release-artifacts/yingliu-merge/task12c3a-target.log`。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，539 个文件、0 项疑似凭据。
- 测试使用本机 Windows 和临时文件；未测试跨进程对抗、完整祖先链、硬链接、重解析点替换、权限变化，未证明 SQLite VFS 实际打开的对象与单独持有的句柄相同。写连接可继续提交不代表所有 checkpoint、重命名和源程序行为都不会受影响。
- 当前实验不能作为真实来源访问的安全门槛；缺失 DB/WAL/SHM 的处理、用户明确选择及读取范围、路径身份绑定仍需设计和测试。本次未打开真实映流资料或迁入用户数据。
- 本阶段只新增测试，生产代码和前端未变化；上一步完整 Rust 回归为 591 通过、2 项既有忽略，前端构建通过，此处未重复全量回归或构建。
