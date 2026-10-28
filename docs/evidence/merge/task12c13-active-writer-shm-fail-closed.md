# Task 12c13：活动写入端下的 SHM 失败闭合

日期：2026-10-28；起点 `3bb6429`。仅 `cfg(test)` 合成快照模块与隔离临时数据库，不提供用户路径入口或修改产品读取器。

- 12c12 残留：`-shm` 已存在但 deny-write（`FILE_SHARE_READ`）租约因活动写入端可写占用而失败时，曾静默回退普通 `READ_ONLY`，读者仍可能改写 SHM。
- 本步改为失败闭合：SHM 存在且租约无法取得时，`open_synthetic_readonly_source` / `snapshot_synthetic` 直接返回错误，**不打开** SQLite，因此不改写主库/WAL/SHM。关闭写入端后，原 deny-write 路径仍可完成 online Backup。
- 活动写入端下若仍需观察已提交 WAL，测试显式走普通只读连接 + `snapshot_synthetic_connection`（标明非 SHM 安全）；该路径不得被 SHM 安全助手使用。未实现 sidecar 拷出或生产授权。
- **不能因此开放真实用户数据库读取。** 无用户路径、无授权、无迁入入口。
- 映流迁入相关隔离测试 38 项通过、退出码 0，日志 `release-artifacts/yingliu-merge/task12c13-target-final.log`。仅测试代码变动，完整 Rust 回归与前端构建沿用此前 12c9 记录（610 通过 / 2 原有忽略）；未读取或迁移真实用户数据。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，571 个跟踪文件、0 项疑似凭据。