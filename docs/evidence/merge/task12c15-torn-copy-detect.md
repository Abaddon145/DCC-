# Task 12c15：拷出路径撕裂检测、失败闭合与静默点重试

日期：2026-10-28；起点 `be4a3d5`。仅 `cfg(test)` 合成快照模块与隔离临时数据库，不提供用户路径入口或修改产品读取器。

- 12c14 已在活动写入端下拷出齐全 DB/WAL/SHM 再对副本快照，但未检测拷贝窗口内字节撕裂。
- 本步为 Windows 拷出路径增加 SHA-256+长度指纹：拷贝前采集源三件套、拷贝后对副本与源再采集；要求 `copied == before` 且 `after == before`。不一致则失败闭合并清理已创建的不完整副本，最多重试 3 次（`SIDECAR_COPY_ATTEMPTS`）。测试钩子 `copy_complete_sidecars_for_snapshot_with_interference` 仅在 `cfg(test)` 注入确定性中途变更。
- RED→GREEN：中途翻转 SHM 后拒绝且不留下 dest 三件套；静默稳定指纹集接受并读到已提交 WAL；源上 `snapshot_synthetic` 失败闭合行为保持。
- **不能因此开放真实用户数据库读取。** 无用户路径、无授权、无迁入入口。指纹门禁降低撕裂拷贝风险，仍不宣称源句柄上实时并发只读已解决。
- 映流迁入相关隔离测试 42 项通过、退出码 0，日志 `release-artifacts/yingliu-merge/task12c15-target-final.log`。仅测试代码变动，完整 Rust 回归与前端构建沿用此前 12c9 记录（610 通过 / 2 原有忽略）；未读取或迁移真实用户数据。
- `cargo fmt --check`、`git diff --check` 通过；凭据扫描脚本 CheckedFiles=557 / PotentialSecrets=0（跟踪文件 573，排除无关的 `Luca/`、`outputs/`、`tmp/`）。
