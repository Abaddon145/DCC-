# Task 12b3：合成固定快照串联检查

日期：2026-10-28；栈藏起点 `2b886da`。串联代码仅位于 Rust 测试模块，不存在用户路径、产品预览或迁入入口。

- 合成清单、所声明文件、SQLite 内存快照和 20 张表的 22 行依次核对，随后复核所声明文件；返回的是检查数量和清单自报的 9 个对象、1 个未解析引用，不表示对象已映射或可迁入。
- 必需或可选文件缺失返回未完成；文件变化、表行不一致、非空 WAL 和阶段间日志变化均拒绝。原始重复 JSON 键在访问根目录之前拒绝。
- 实际合成 SQLite 写入的 WAL-only 笔记负例中，主文件摘要保持不变，但固定快照检查仍拒绝；检查前后合成 DB/WAL 字节未变。活动 WAL 的在线快照行为另由 Task 12b1 测试。
- 正例检查期间合成主文件和独立目标 SQLite 哨兵内容未变；这不是完整目标素材库或真实用户数据验证。SQLite 可能创建空 WAL/SHM，本测试只拒绝非空 WAL，不声称整个源目录零变化。

## 测试和边界

- 新能力及原始字节接口先出现缺失函数的红灯，再实现；定向 `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_preflight -- --test-threads=1`：6 通过。
- 完整 `cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`：562 通过、0 失败、2 既有忽略，main/doc 测试通过，退出码 0。日志 `release-artifacts/yingliu-merge/task12b3-rust-final.log`。
- `cargo fmt --check` 和 `git diff --check` 通过；凭据扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，528 个文件、0 项疑似凭据。
- 文件可能在两次检查之间替换再恢复，硬链接及真实 Windows 句柄竞争尚未解决；未核验未声明业务表、列约束及真实来源权限。固定合成快照不可代替真实迁入预览。
- 前端及生产编译模块无变化，沿用 Task 12a 的生产构建结果；未访问真实映流资料，未推送、合并、发布、安装或调用付费 AI。
