# Task 12c12：关闭写入端后的 SHM 只读控制

日期：2026-10-28；起点 `e35be4e`。仅 `cfg(test)` 合成快照模块与隔离临时数据库，不提供用户路径入口或修改产品读取器。

- 复现 12c11 残留：写入端退出后，普通 `SQLITE_OPEN_READ_ONLY` 仍会改写已有 `-shm` 字节；主库与 WAL 字节可保持不变。
- 控制路径：若 `-shm` 已存在，先以 `FILE_SHARE_READ`（禁止他人写入）持有该 sidecar，再普通只读打开并进行 online Backup；租约必须活过连接。关闭写入端后，主库/WAL/SHM 字节均不变，且 Backup 仍能读到 WAL 已提交行。
- 实测 `readonly_shm=1` URI 也能在查询路径下保住 SHM，但在本仓库捆绑的 SQLite 上会使 online Backup 长时间 Busy/More（探针超时），因此不用于合成快照路径。
- 若活动写入端已可写持有 SHM，则 deny-write 租约打开失败并回退为普通只读；此时 SHM 仍可能被改写。该活动写入竞争仍属残留，不能当作生产安全开关。
- **不能因此开放真实用户数据库读取。** 无用户路径、无授权、无迁入入口。
- 映流迁入相关隔离测试 36 项通过、退出码 0，日志 `release-artifacts/yingliu-merge/task12c12-target-final.log`。仅测试代码变动，完整 Rust 回归与前端构建沿用此前 12c9 记录（610 通过 / 2 原有忽略）；未读取或迁移真实用户数据。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，570 个跟踪文件、0 项疑似凭据。