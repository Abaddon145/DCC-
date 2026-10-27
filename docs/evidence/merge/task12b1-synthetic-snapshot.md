# Task 12b1：隔离 SQLite WAL 快照

日期：2026-10-27；栈藏起点 `ce12a86`。新增代码仅在 Rust 测试构建启用，不存在用户路径选择、迁入命令或界面入口；全部数据库由测试在临时目录创建。

## 实施及边界

- 用 SQLite `READ_ONLY` 连接和现有 rusqlite online backup 把合成库复制到内存；禁用源连接关闭时的 checkpoint，复制前将内存库 page size 设为源值。`Busy/Locked` 失败，分步复制的循环有时间检查，不把部分结果返回为成功。
- 快照检查 integrity、外键、`schema_version` 表唯一的一行 `8` 和 `PRAGMA user_version=8`。锁定测试只证明锁源及时失败，未分别证明 `Busy` 和 `Locked` 两种返回路径。时间检查不约束操作系统 I/O 或完整性检查的硬截止。
- SQLite 可能建立或维护 `-shm`；本步骤只在受控测试中观察业务 DB/WAL 的字节前后不变，不声称源目录绝对没有写入，也不代表实际用户目录的句柄、ACL 和竞争问题已解决。

## 测试

- 首次定向运行因模块不存在失败；补齐功能后发现测试夹具使用会返回结果的 PRAGMA 与默认外键配置不匹配，修正夹具后通过。
- 定向 `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_snapshot -- --test-threads=1`：4 通过、0 失败。覆盖仅在活动 WAL 的两条记录、单独主文件遗漏对照、8192 页、源写连接继续使用、最后读取者关闭后 DB/WAL 字节、错误版本/重复行、无效外键、无效 SQLite 文件、缺文件和锁源失败。
- 完整 `cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1` 的日志显示 545 通过、0 失败、2 既有忽略，main/doc 测试结果均通过；控制会话在用户继续消息后失联，未另记录 shell 退出码。日志 `release-artifacts/yingliu-merge/task12b1-rust-full.log`。
- 前端及生产编译模块无变化；沿用 Task 12a 的生产构建结果。独立只读审查确认上述边界；未将测试中的非法文件头夸大为所有内部损坏类型的覆盖。
- `cargo fmt --check` 和 `git diff --check` 通过；来源扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，517 个文件、0 项疑似凭据。

## 未完成

尚未核验合成契约的真实 20 张表、行及外部文件，未检测读取过程中真实源变化，未建立用户许可/来源身份，也未提供迁入或恢复。未访问真实映流数据，未推送、合并、发布、安装或调用付费 AI。
