# Task 12b2a：合成 SQLite 表内容核对

日期：2026-10-27；栈藏起点 `836bb17`。代码仅在测试构建启用，无用户目录选择、迁入命令、页面或业务数据写入。

## 范围

- 合成样例数据库复制前后的 SHA-256 为 `dba49e0512521ce4f4f859a90e013c394dd6846858cc7c23b1c10e4140692723`；只在临时目录副本上打开，未用 SQLite 打开映流仓库或真实用户数据库。
- 复用清单形状检查与内存快照，比较 20 张声明表的普通表类型、列名集合和 22 行值；排序后比较行多重集，保留重复数量及 NULL、整数、实数、UTF-8 文本类型。布尔、无穷值、BLOB 和不安全标识符不作为有效源值。
- 独立核查发现原先 `sqlite_schema.type='table'` 也包含虚拟表；构造列名和行值完全匹配的 RTree 表后测试确实失败，再使用 `pragma_table_list` 区分普通表与虚拟表并通过测试。
- 这里只检查列名与值，不验证列声明类型、默认值、索引、触发器或约束，也不枚举未声明业务表；不验证外部文件、实际文件身份或来源授权。校验样例输入字节不是对未来在线快照文件进行摘要核验。

## 测试与检查

- 红灯：初始测试因模块不存在失败；虚拟表负例之后复现“被误认为普通表”，修正后通过。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_table_check -- --test-threads=1`：5 通过、0 失败。包含行/列重排、NULL 与空串、整数与实数/布尔、源数据库 BLOB/REAL、重复行、列改名或增列、虚拟表及恶意字段名。
- 完整 `cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`：550 通过、0 失败、2 既有忽略；main/doc 测试通过，命令退出码 0。日志 `release-artifacts/yingliu-merge/task12b2a-rust-final.log`。
- `cargo fmt --check` 和 `git diff --check` 通过；前端与生产编译模块未改，继续沿用 Task 12a 的生产构建。独立只读审查核对了类型、排序和 SQL 标识符边界。
- 来源扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，521 个文件、0 项疑似凭据。

## 未完成

外部文件校验、读取过程源变化、来源身份/用户许可、迁入预览、正式执行及失败恢复仍待实施。未访问真实映流数据，未推送、合并、发布、安装或调用付费 AI。
