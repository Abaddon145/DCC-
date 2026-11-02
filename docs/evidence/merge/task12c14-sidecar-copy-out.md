# Task 12c14：活动写入端下的完整 sidecar 拷出快照

日期：2026-10-28；起点 `2872753`。仅 `cfg(test)` 合成快照模块与隔离临时数据库，不提供用户路径入口或修改产品读取器。

- 12c13 已要求：源上 deny-write SHM 租约失败时失败闭合，禁止静默普通只读。活动写入端下若仍要读已提交 WAL，不能再打开源库。
- 本步增加 Windows 拷出路径：在 `FILE_SHARE_READ|FILE_SHARE_WRITE`（不共享删除）下逐字节复制已存在的 DB/WAL/SHM 到隔离目录，再对**副本**调用原有 SHM deny-write + online Backup。SQLite 不打开源文件，因此本读者不会改写源 SHM；写入端仍可继续提交。
- 任一 sidecar 缺失则拒绝拷出且不创建缺失文件。撕裂拷贝（写入过程中的不一致字节）仍属残留，测试在已提交静默点拷贝；不能宣称实时并发 RO 已在源句柄上解决。
- **不能因此开放真实用户数据库读取。** 无用户路径、无授权、无迁入入口。
- 映流迁入相关隔离测试 40 项通过、退出码 0，日志 `release-artifacts/yingliu-merge/task12c14-target-final.log`。仅测试代码变动，完整 Rust 回归与前端构建沿用此前 12c9 记录（610 通过 / 2 原有忽略）；未读取或迁移真实用户数据。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，573 个跟踪文件、0 项疑似凭据。