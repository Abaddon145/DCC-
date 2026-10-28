# Task 12c10：Windows SQLite 文件句柄实验

日期：2026-10-28；起点 `f05f511`。仅测试代码和隔离临时数据库，没有修改产品读取器或读取用户资料。

- SQLite 的 `SQLITE_FCNTL_WIN32_GET_HANDLE` 可获取连接实际使用的主库句柄；通过 `SQLITE_FCNTL_JOURNAL_POINTER` 和该文件对象的 `xFileControl` 可获取已打开的 WAL 句柄。合成测试分别将主库和 WAL 与已持有文件的卷序列号、文件索引核对，并验证错误文件不匹配；并行写入后独立 SQLite 快照包含新记录。
- 反例验证：`SQLITE_FCNTL_JOURNAL_POINTER` 返回成功并不代表 WAL 已打开；无活动 WAL 时文件方法指针为空。实际代码必须检查指针及返回值，不能关闭 SQLite 拥有的句柄。
- 此接口不能证明 SQLite 从未先打开未经核验的 WAL 或 SHM：WAL 在读取过程中可能已被打开；公开接口未提供 SHM 底层句柄。原先不存在的侧文件也可能在检查后被创建，不能把本实验作为完整路径安全证明。真实来源读取仍未启用。
- 定向测试 2 项通过，退出码 0，日志 `release-artifacts/yingliu-merge/task12c10-target-final.log`。本阶段仅增加测试；完整 Rust 回归沿用上一阶段的 610 通过、2 项原有忽略，前端构建沿用上一阶段结果；未重复运行。
- 独立只读核查使用本机 SQLite 官方源代码，不涉及外部项目代码复制。真实 SQLite 路径授权、受控 sidecar 打开方式及实际用户数据迁入仍待完成。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，552 个文件、0 项疑似凭据。
