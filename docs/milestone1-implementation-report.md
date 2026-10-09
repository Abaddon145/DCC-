# 栈藏：里程碑 1 实施与验收报告

日期：2026-10-10。范围：用户批准的 A1、A2、A5、A3、A4；基线为 `88991de03217e35694e6c30d7496b237c04baf14`，产品版本仍为 **v0.20.6**。

## 交付内容

| 任务 | 结果 | 累计分支 / 提交 |
| --- | --- | --- |
| A1 | Fab 仅采用 UE 格式的软件、版本及格式信息；其余格式明确提示并进入单条/批量结果。重读失败或仅修改待读取网址，不丢失已填入商品的警告。 | `milestone1/A1` / `d28a667dc38a93186c74c0fc234e2e17c314b7f9` |
| A2 | 定位前检查 Epic 主进程；未运行时提示手动启动，不自动启动。检测失败保留原导航行为；普通定位与显式恢复路径不变。 | `milestone1/A2` / `10218d8e681a6768e3cdca2a56c475d2ef8963d5` |
| A5 | 合并六个测试版本的变更摘要，README 保留当前版本摘要及有效功能说明。 | `milestone1/A5` / `529ddec80788c7aa82fa40546fd5e1d3bcd7f301` |
| A3 | 单条保存及批量补齐前提供只读费用预览和确认；错误/超限可单条仅保存原文，已有内容不覆盖。 | `milestone1/A3` / `521fb22bab1d839a516988e7d838de978b13ab79` |
| A4 | 每个既有库首次由当前应用版本打开时，迁移前生成并校验元数据库快照；确认恢复中断原库时也先保护再迁移。 | `milestone1/A4` / 本报告所属 A4 提交，完整 SHA 记录于草稿 PR 描述 |

本报告在 A4 提交中，不能自引用该提交的哈希。新草稿 PR 的描述列出最终五个完整 SHA。目标为 `milestone1/A4 → codex/v0.20.1-single-import-epic`，原 PR #13、原分支不变。仅推送新分支和新草稿 PR，不合并、不发布、不生成安装包。

## 文件清单

- A1：`src-tauri/src/{fab,commands,models}.rs`；`src/types.ts`；`src/components/AssetEditor.tsx`、`AssetEditor.test.tsx`；批准的设计与实施计划 `docs/superpowers/{specs,plans}/2026-10-09-milestone1*.md`。
- A2：`src-tauri/src/epic.rs`。
- A5：`CHANGELOG.md`、`README.md`、`src/lib/changelog.test.ts`。六份历史版本原文未改动。
- A3：`src-tauri/src/{translation,translation_service,commands,models,lib}.rs`；`src/{App.tsx,types.ts,styles.css}`；`src/lib/api.ts`；`src/components/{AssetEditor.tsx,AssetEditor.test.tsx,TranslationCostDialog.tsx,TranslationCostDialog.test.tsx}`。
- A4：`src-tauri/src/{upgrade_backup,state,lib,backup,reliability}.rs`；`src/types.ts`、`src/lib/api.ts`；`src/components/{ReliabilitySettings.tsx,ReliabilitySettings.test.tsx}`；本报告。

## 验证结果

| 检查 | 基线 | 最终实现 |
| --- | --- | --- |
| 前端测试 | 52 文件，250 通过 | 55 文件，267 通过 |
| Rust 单元测试 | 197 通过，2 既有忽略 | 237 通过，仍仅 2 既有忽略 |
| Rust 格式检查 | 通过 | 通过 |
| TypeScript + Vite 生产构建 | 通过 | 通过 |
| 差异空白检查 | 通过 | 通过 |

执行：`npm test`、`cargo test --locked --lib`、`cargo fmt --check`、`npm run build`、`git diff --check`。凭据和真实在线服务不参与 CI。保留原有链接器提示及大于 500 kB 的构建分块警告，未添加忽略项。

安装依赖时普通权限出现 EPERM，原命令获批后成功；格式检查也有一次沙箱路径访问失败，获批同命令通过。`npm ci` 报告已有 1 项高风险依赖问题；本里程碑不升级依赖或执行自动修复。

A3 后一次已有 `creative_auth_failure_is_safe_and_not_retried` 测试间歇失败；单独及全量重跑均通过，未改该测试或实际翻译逻辑。根因未证明，后续宜单独调查本地模拟 HTTP 夹具。

版本文件、依赖及锁文件、schema 和 `db.rs` 均未修改；未重写历史素材、任务日志格式或真实翻译额度预留/结算/降级逻辑。所有审查修复归入对应任务提交。整理提交时两处测试重叠保留双方行为，整理后产品 Git tree 与先前全量验证版本完全一致。

## 新测试名称

共新增 40 项 Rust 测试及 17 个前端测试实例。下列参数化前端测试展开为三个实例；另为已有“手动改变来源”用例加入格式警告，确认它会被正确清除。

### A1

- `fab_mixed_formats_only_adopts_unreal_information`
- `fab_format_warning_is_unique_and_absent_for_pure_ue`
- `single_save_retains_fab_format_warning`
- `bulk_format_warning_enters_row_messages`
- `single_save_rejects_untrusted_fab_warning_before_writing`
- `keeps the applied format notice after %s and passes it with save, not asset data`：`successful read`、`failed reread`、`unapplied read URL`。

### A2

- `launcher_csv_matches_target`
- `launcher_empty_output_is_not_running`
- `launcher_localized_no_tasks_is_not_running`
- `launcher_process_name_is_case_insensitive`
- `launcher_web_helper_is_not_target`
- `launcher_not_running_prevents_both_routes`
- `launcher_detection_failure_preserves_navigation`

### A5

- `changelog_covers_six_versions_in_descending_order`
- `readme_has_latest_version_summary_and_valid_changelog_link`
- `changelog_excludes_installer_and_checksum_details`

### A3

- `cost_all_cached_is_zero_and_usage_unchanged`
- `cost_uses_request_reservation_and_rejects_over_limit`
- `cost_unconfigured_and_disabled_make_no_online_requests`
- `cost_baidu_only_and_fully_cached_ignore_deepseek_existing_overage`
- `cost_cache_disabled_and_current_blocks_match_actual_policy`
- `cost_terms_and_exact_tags_are_local`
- `cost_long_text_matches_live_fragment_batches`
- `cost_ignores_existing_localizations_and_deduplicates_ids`
- `cost_wrong_library_rejects_without_usage_change`
- `cost_form_disabled_matches_real_save_switch_and_manual_edits`
- `single_save_skip_online_keeps_original_and_category`
- `shows_pending_fields_cache_and_reserved_interval`
- `ignores_stale_estimate_on_input_or_library_change`
- `keeps_unread_fab_out_of_online_estimates_and_can_save_raw`
- `combines_chunks_without_counting_monthly_usage_twice`
- `disables_online_when_estimate_exceeds`
- `shows_no_online_request_when_disabled`
- `retains_raw_save_after_cost_failure`
- `prevents_duplicate_confirmation_submit`

### A4

- `upgrade_backup_same_version_skips`
- `upgrade_backup_new_version_once`
- `upgrade_backup_integrity_failure_keeps_file_and_version`
- `upgrade_backup_keep_only_removes_registered_oldest`
- `upgrade_backup_retention_uses_registered_time_not_vector_order`
- `upgrade_backup_outside_and_unregistered_candidates_are_untouched`
- `upgrade_backup_rejects_linked_backup_directory`
- `upgrade_backup_precedes_migrations`
- `upgrade_backup_second_library_and_copied_uuid_are_protected`
- `upgrade_backup_new_library_skips`
- `upgrade_backup_restore_interruption_skips`
- `upgrade_backup_explicit_restore_precedes_migrations`
- `upgrade_backup_failure_does_not_block_open`
- `upgrade_backup_settings_failure_retries_next_open`
- `upgrade_backup_old_settings_defaults_and_invalid_keep_clamps`
- `upgrade_backup_commands_are_bound_and_keep_validation_is_atomic`
- `upgrade_backup_restore_allows_read_but_rejects_keep_changes`
- `shows_upgrade_snapshot_separately_from_full_backup`
- `validates_upgrade_backup_keep_between_one_and_twenty`
- `ignores_status_from_previous_library_and_can_retry_read`

## 费用估算边界

估算不联网、不新增缓存、不写用量，也不改变实际额度保护。精确术语标签和已有缓存不计在线请求；同一估算内重复请求去重。按实际保护、分段、1,800 字符分组及服务顺序投影；DeepSeek 正常请求上界采用既有预留公式：序列化请求体字节数加 512 加 4,096，区间下界为 0。模拟 `one\ntwo` 请求的估算上界 5,445 与实际正常预留相等。

这是本机额度预留区间，不是人民币价格或保证的实际账单。重试、修复响应、备用服务以及前序成功后才可复用的暖缓存会带来不确定性，界面有明确提示。翻译和创作共享的月度已用不重复累加。仅本次确有 DeepSeek 请求才按其额度判断；纯百度请求或全部缓存命中不因既有 DeepSeek 超额而禁用。

未读取的 Fab 表单必须先读取元数据，才可确认完整补译；仍可仅保存原文。未配置的待在线翻译显示错误，不伪装为零费用。旧库、旧输入的迟到响应丢弃。

## 快照与恢复边界

快照位置为素材库同级 `升级前自动备份/<规范化路径 SHA-256>/`。默认保留 5 份，可设 1–20；只删除本功能登记且通过路径、文件名、UUID、所属目录及链接检查的旧成功快照，不删除手动文件或其他库文件。失败文件保留，记录持久化失败下次重试；警告不阻断仅因保护失败的正常打开。

**元数据库快照不是完整备份，不包含图片、音视频、模型或笔记附件。** 原有完整 `.dccassetlib` 包、默认策略（关闭 / 24 小时 / 3 份）及恢复方式保持不变。没有添加裸数据库恢复入口。恢复中断时启动不读、不迁移半成品数据库；明确确认恢复原库后，先回滚文件，再执行共享保护和现有迁移。

## 独立审查与裁决

一次独立只读 `gpt-6-astra` 全分支审查发现两项 Important，无 Critical 或 Minor。两项均先以测试复现失败，再修复并全量通过：Fab 警告被重读/未应用网址清除；明确恢复原库绕过升级前保护。不发起第二次审查，不将作者自查描述为独立验收。

以下为执行期间全部裁决及误判成本，按发生顺序：

1. 原生隔离工作区在可写权限范围外，采用技能允许的主目录、新累计分支回退；原分支不变。成本：文件系统隔离较弱，需继续保护无关改动。
2. 技能脚本使用获批 Git Bash 执行，未自造替代脚本。成本：额外授权等待。
3. 文档测试检查结构、链接及排除内容，不逐字绑定文案。成本：少量文档结构测试维护。
4. 未配置且有在线待译内容时明确返回估算错误，保留单条原文保存。成本：在线确认前需配置服务。
5. 保留私有、仅测试用的保存选项适配器，生产使用统一实现。成本：一个小型测试适配层。
6. 完整性失败注入复用私有函数指针；真实路径仍只读校验。成本：一个私有测试接缝。
7. 原完整备份设置也复用请求身份保护，避免切库后的迟到设置覆盖。成本：小范围设置读取变化，不改存储或调度。
8. 现有手动字段翻译不在本次确认入口范围，保留行为；新增确认用于保存/批量补齐。成本：手动翻译暂不具有新费用确认界面。
9. 安装环境 Epic 导航及手工快照恢复不冒充已验收，保留为人工检查。成本：环境特有问题可能需要后续修复。
10. 明确恢复原库需在迁移前保护，补充 `backup.rs`、`reliability.rs` 的窄集成；不改备份格式或调度。成本：额外的恢复集成路径。
11. 用户已批准推送新草稿 PR，直接执行该选择，不再次询问合并菜单。成本：交付停留在待评审草稿，不自动合并发布。

无延后 Minor 项。

## 凭据扫描

限定 `git ls-files` 的已跟踪源码/配置/文档，`rg -l` 只报告匹配文件，不输出疑似值。四个模式均先以合成假值证明能匹配，再扫描：

```text
-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----
(ghp_|github_pat_)[A-Za-z0-9_]{20,}
sk-[A-Za-z0-9_-]{20,}
(?i)(api_?key|secret_?key|password|access_?token)\s*[:=]\s*[\x22\x27][^\x22\x27\r\n]{4,}[\x22\x27]
```

四组结果均为 0 个匹配文件。扫描不能证明不存在所有形式的秘密；Windows 凭据管理器、本机数据与未跟踪产物不进入交付。

## 尚待人工验收

本次没有安装包，也没有在已安装的 Windows 软件环境中完成以下验收：

1. 完全关闭 Epic，普通/恢复定位都应提示先启动，不能自动打开。手动启动登录后连续定位两件不同商品：普通定位不经过 UE 库，显式恢复保留交接。成功提示仅表示已发出请求。
2. 单个已购 Fab 商品读取、保存与多选补齐分别查看费用确认；超限/配置错误可单条保存原文；确认已有中英内容、手选分类和忽略格式警告保留。
3. 在测试库副本改变保护版本记录，核对迁移前快照旧 schema、完整性、同版本跳过及第二个旧库独立保护；确认未复制媒体。
4. 在测试副本模拟恢复中断，启动不得读取半成品 DB；确认恢复原库后，旧元数据库快照先于迁移。
5. 核对保留数量、失效权限警告、设置写失败后的重试和切库期间迟到响应。
6. 裸 DB 手工恢复仅在关闭软件、保留完整库副本并处理 DB/WAL 后于测试副本进行；禁止覆盖唯一的真实用户库。完整包仍走原有恢复入口。

以上不新增功能方向，不追溯变更历史素材；后续工作应在本里程碑草稿 PR 评审后另行确定。
