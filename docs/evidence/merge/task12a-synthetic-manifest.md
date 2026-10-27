# Task 12a：合成清单声明形状检查

日期：2026-10-27；栈藏起点 `8dd9020`。本步仅在 Rust 测试构建中启用内部检查器，无生产命令、页面、迁入或文件读取入口。

## 输入和范围

- 固定样例复制自映流 `docs/integration/yingliu-migration-input-v1/sample-manifest.json`，复制前后 SHA-256 均为 `5618b1423558f3b40759886d93dde207802741458cfec305117d076cbf1a98db`。该样例明确为 `synthetic=true`，其 `sampleRootHint` 未用于打开文件。
- 检查输入 JSON 各层重复键、大小上限、声明的 v1/schema 8 标志、规范来源 UUID、20 个声明表的行列形状、声明摘要的一致性、结构化来源键唯一性以及明显不安全的相对路径和大小写碰撞。
- 返回值均命名为 `declared_*`。故意伪造快照摘要和字节数的测试仍可通过形状检查，证明该步骤**没有**把声明当作文件核验；未知目标类型不会被标记为已映射。
- 16 MiB 仅约束本测试阶段的合成清单；不是未来真实迁移的大小承诺。字符串路径检查不取代 Windows 句柄身份、重解析点检查或用户授权。

## 验证

- 红灯：新增测试引用尚不存在的内部模块，Rust 定向编译因缺少 `import_manifest` 失败。
- 复核后定向测试：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_manifest -- --test-threads=1`，5 通过、0 失败。
- 完整 Rust 回归：`cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`，541 通过、0 失败、2 既有忽略；main 和 doc 测试通过。日志 `release-artifacts/yingliu-merge/task12a-rust-final.log`。
- `npm run build` 退出 0；既有大 chunk 提示仍在。此后仅修改 Rust 测试专用模块及测试名称，未修改前端或生产编译模块。`cargo fmt --check` 和 `git diff --check` 通过。
- 来源扫描排除无关的 `Luca/`、`outputs/`、`tmp/`，514 个文件、0 项疑似凭据。

## 未完成

未验证真实文件、SQLite 真实列及内容、活动 WAL、Windows 句柄和竞争、来源选择授权、真实用户数据、目标库写入或迁入恢复。本步不提供用户可见的迁入功能；Task 12b–e、Task 13 和真实 Windows 最终验收仍待实施。未推送、合并、发布、安装或调用付费 AI。
