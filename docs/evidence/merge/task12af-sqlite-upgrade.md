# SQLite WAL 依赖升级

2026-11-01。目标应用的 `rusqlite` 从 0.32.1 升级到 0.41.1，内置 `libsqlite3-sys` 从 0.30.1 升级到 0.39.1。实际运行时版本测试报告 SQLite 3.54.1；依赖树只包含一个 `libsqlite3-sys`。启用 `fallible_uint` 保持无符号整数的受检转换，并适配新版列名迭代器和测试用进度回调接口。

旧版内置 SQLite 3.46.0 在上游记录的 [WAL 重置缺陷](https://sqlite.org/wal.html#the_wal_reset_bug)影响范围内，且早于[只读 WAL 异常页大小导致越界读取的修复](https://sqlite.org/bugs/forumpost/bb53b6794f99865e51e2a56157c070949d24081a7fa82fecda2ac27a0bb64064?t=h)。新版包含这些上游修复；本项目未复现上述缺陷，也没有据此认定任何用户数据受损。此升级只影响目标应用内置的 SQLite，不改变映流来源应用使用的版本。

完整后端回归 767 项通过、2 项忽略；随后分别通过内置版本门槛和无符号数越界转换测试。Rust 1.98.1 及项目声明的最低 Rust 1.89.0 下，发布配置 `cargo check --release --locked --offline` 均通过；这是编译检查而非发布构建。记录见 `release-artifacts/yingliu-merge/sqlite-upgrade-full.log`、`sqlite-upgrade-targeted.log`、`sqlite-upgrade-release-check.log` 和 `sqlite-upgrade-msrv-check.log`。本次未重复前端测试。

为验证最低版本，安装了 Rust 1.89.0 工具链；默认工具链仍为 stable。安装过程中 rustup 自动从 1.29.1 更新到 1.29.2。

真实来源读取仍未开放：依赖升级不能证明来源数据库及其 WAL、共享内存文件在并发变化时的快照安全。未读取真实用户数据库、未完成原生桌面验收、未合并 main。
