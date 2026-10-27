# DCC 学习资料只读迁入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` inline，逐步测试、记录并提交。用户已批准当前会话顺序实施；真实用户数据迁移、凭据解密、覆盖安装、合并与发布需另行授权。

**Goal:** 为栈藏接入可核验、可恢复的映流学习资料预检与迁入，保留已有素材、笔记和来源上下文。

**Architecture:** 栈藏仍是唯一主应用，复用当前媒体、笔记和任务系统；迁入前独立核验来源身份、SQLite 一致性快照和外部文件，再生成明确的冲突预览。先以双方约定的合成清单建立内部格式检查，不能将清单中自称的摘要、路径和来源身份当作真实文件核验或读取授权。当前已有 schema 30，确需迁入持久记录时使用下一版本而不覆盖 v29/v30。

**Tech Stack:** Rust、rusqlite 0.32 的现有 backup 功能、serde、Windows 文件身份；后续 UI 使用现有 React 和任务中心，不引入第二数据库或依赖。

**Spec:** `C:/Users/14582/Documents/ChatGPT/视频下载软件/docs/superpowers/specs/2026-10-21-stackdcc-yingliu-merge-design.md` §6.3；`C:/Users/14582/Documents/ChatGPT/视频下载软件/docs/integration/yingliu-migration-input-v1.md`；本仓库主计划 `docs/superpowers/plans/2026-10-21-stackdcc-yingliu-merge.md` Task 12–14。

## Global Constraints

- 只改栈藏仓库，不改映流仓库及两端原始方案；不读取真实用户来源数据，不复制凭据、Cookie，不触发付费 AI。
- 本阶段合成资料只验证实现，不构成真实数据迁移授权或双应用、Windows 物理验收。
- `sampleRootHint` 与清单来源 ID 不是路径读取授权。无用户明确选择和来源绑定，不接受可由清单决定的磁盘根。
- 不使用源端会执行迁移或改 `journal_mode` 的服务构造函数；不得强制结束源软件或 checkpoint 源 WAL。
- 笔记正文、同名对象、同视频多系列和目标修改都不能静默覆盖或合并；源 Vault 原字节需保留。
- 独立阶段测试、记录并本地提交；不自动推送、合并、发布、安装或迁入真实用户库。

## Review Focus

1. 畸形 JSON、重复键或超大输入：明确拒绝且不读取任何路径；Task 12a 测试。
2. 路径穿越、Windows 设备名及大小写碰撞：不能作为受信文件定位；Task 12a 测试语法，12b 再核验句柄与授权。
3. 清单自称来源／摘要一致但文件或 SQLite 已变化：不能当成实物核验；Task 12a 只返回结构摘要，12b 对合成文件逐项复核。
4. 活动 WAL、半写入源、源和外部 Markdown 不同时冻结：生成一致快照并在发布前复核，忙或变化则失败；Task 12b 测试。
5. 重复执行、目标被用户修改、旧路径失效或中断：按结构化来源键和目标修订报告冲突，不覆盖；Task 12c–e 测试。

## Task 12a：合成输入清单的纯格式检查

**Files:** 新 `src-tauri/src/learning/import_manifest.rs`、`import_manifest_tests.rs`、`fixtures/yingliu-migration-sample.json`；仅在 `learning/mod.rs` 注册内部模块及测试。**无 Tauri 命令、UI、文件读取、数据库写入或迁移执行入口。**

**Interface:** `validate_synthetic_manifest(&[u8]) -> Result<SyntheticManifestSummary, String>`；只接收内存字节并报告结构计数。使用合成协议 `format/version/synthetic`、规范 profile UUID、snapshot 两个 schema 8 标记与 20 个表形状、声明文件摘要和结构化来源键做保守检查。JSON 所有层级拒绝重复键；输入最多 16 MiB，该上限仅用于当前合成检查，不能据此限制最终真实迁移。未知映射不得被视为导入成功；即使通过也不证明任何文件、SQLite、实际关联或权限可信。

- [x] 复制协议方已发布的合成 JSON 样例并记录 SHA-256；样例仅供固定测试，不能把其中路径当产品入口。
- [x] 先写测试：样例通过；坏版本/schema/profile、重复 JSON 键和来源键、缺列/行形状、损坏摘要、非法路径/大小写碰撞、超限/坏 UTF-8 均拒绝。运行定向测试，确认新能力缺失而失败。
- [x] 使用现有 serde/uuid 和标准库实现检查；所有错误是通用中文消息，不回显输入内容、外部路径或密钥；不解析、访问或信任 `sampleRootHint`。
- [x] 定向测试、完整 Rust 回归、构建、格式与凭据检查；写证据与实际边界并独立提交。详见 `docs/evidence/merge/task12a-synthetic-manifest.md`。

## Task 12b1：隔离 SQLite 活动 WAL 快照

**Files:** 新 `src-tauri/src/learning/import_snapshot.rs`、`import_snapshot_tests.rs`；仅在 `learning/mod.rs` 的测试构建注册。仍无用户路径、Tauri 命令、目标库写入或迁移入口。

**Interface:** 合成测试的只读连接生成独立内存快照；调用 rusqlite 已启用的 online backup，限制等待时间，不执行源迁移、checkpoint、journal_mode 写入或调用源服务构造函数。快照只验证 integrity、外键以及 `schema_version` 与 `user_version` 均为 8；该步骤不声明 20 张表、外部文件或生产路径已核验。

- [x] 在临时合成库中保持 WAL 写入连接打开、关闭自动 checkpoint、提交只存在 WAL 的笔记；确认裸主库看不到但快照能看到，读取前后合成 DB/WAL 字节不变，写连接仍可使用。
- [x] 合成库 schema 版本不支持、数据库损坏、外键不一致或读取被锁时明确失败；锁等待有上限且失败无半份成功快照。
- [x] 红灯后实现最小只读快照；只针对测试临时目录，不把路径检查误认为用户授权。测试记录 SQLite 可能操作测试库的 `-shm`；真实 Windows 的句柄、ACL 与源文件竞争尚未通过该测试。
- [x] 定向与完整 Rust 回归、格式/凭据检查，写证据并独立提交。文件摘要、正式来源许可及 manifest 与 SQLite 的行一致性留给 12b2/12c。详见 `docs/evidence/merge/task12b1-synthetic-snapshot.md`。

## Task 12b2a：合成清单与数据库逐表核对

**Files:** 新 `src-tauri/src/learning/import_table_check.rs`、`import_table_check_tests.rs`、`fixtures/yingliu-migration-sample.sqlite`；`learning/mod.rs` 仅测试构建注册；复用 12a 清单形状检查和 12b1 内存快照。无生产路径、UI 或迁入入口。

**Interface:** 对调用方已生成的内存快照和合成 JSON 比较 20 个固定表；逐个确认 SQLite 普通表及 `table_xinfo` 列集合，显式、安全引用列名，忽略列序。按保留重复数量的行多重集比较，保留 NULL/整数/实数/UTF-8 文本类型，拒绝布尔、非有限浮点和 v1 未定义编码的 BLOB。返回已核对行数，不把未知表、外部文件或授权视为已核验。

- [x] 核对并固定双方提供的合成 SQLite 样例 SHA-256，复制到测试夹具；只在测试 TempDir 副本上打开，绝不打开来源工程或真实用户数据库。
- [x] 先测真实样例、行/列重排、NULL 与空串、整数与实数/布尔、重复行、内容变动、缺列/额外列、BLOB 和不安全列名；确认缺少能力时 RED。
- [x] 实现固定表名与显式列查询、按类型和多重集比较；不一致明确失败，不把 manifest 路径当权限。列和行完全匹配的 SQLite 虚拟表也必须拒绝。
- [x] 定向和完整 Rust 回归、格式/凭据检查及独立本地提交。文件身份/外部文件、未声明对象、来源许可和恢复仍属后续步骤；证据见 `docs/evidence/merge/task12b2a-table-parity.md`。

## Task 12b2b：合成原件和附件内容核对

**Files:** 新 `src-tauri/src/learning/import_file_check.rs`、`import_file_check_tests.rs`、少量 `fixtures/yingliu-files/` 合成样例；`learning/mod.rs` 仅测试构建注册，`.gitattributes` 禁止对固定样例换行转换。复用 12a 合成清单检查和 12b2a 快照夹具；长文在临时目录按原样生成并核对摘要。

**Interface:** 测试内部以显式临时根接收合成清单，核验其中声明的 snapshot 和文件的大小、SHA-256 与两次读取的一致性；区别必需及可选文件缺失，不读取 `sampleRootHint`。只返回计数/缺失状态，不提供文件内容或生产路径权限。8 MiB 合成夹具读取上限不是对真实媒体的上限；生产所需授权、文件句柄身份和重解析竞争仍需独立设计。

- [x] 固定双方合成小文件并检查 SHA-256；测试在临时目录构造完整快照、图片、Markdown 和伪视频，不接触真实文件。
- [x] RED→GREEN：完整文件与快照、必需/可选缺失、内容/长度/摘要变动、重复路径和越界路径、源根缺失及过大样本；错误不泄漏文件内容。
- [x] 读取过程复核大小、修改时间、双次 SHA 和规范相对路径；Windows 测试使用只读共享方式，仍不声称消除所有真实路径替换竞争。
- [x] 定向和完整 Rust 回归、格式/凭据检查，写证据并独立提交；未声明文件、活动写入与实际授权留待后续阶段。证据见 `docs/evidence/merge/task12b2b-file-parity.md`。

## Task 12b3：合成固定快照的串联验证

**Files:** 扩展 `src-tauri/src/learning/import_file_check_tests.rs`；在测试模块内直接组合已有清单、文件、内存快照及表核对。无新增生产入口或真实路径读取。

**Interface:** 合成导出样例只作为固定快照验证；先检查同一份清单，再验证所有声明文件、检查 WAL、读取同一内存快照并对比声明表，最后重复检查文件与 WAL。任一必需或可选文件缺失显示未完成；过程中变化拒绝。报告区分实际匹配数与清单自报的对象/未解析引用，不报告“可迁入”。样例活动非空 WAL 与主文件摘要不构成一个完整导出，固定快照检查必须拒绝；活动 WAL 的一致性快照行为由 12b1 单独验证。

- [x] RED→GREEN：完整样例、必需/可选缺失、同长度文件变化、表行变更、阶段间文件变化、非空 WAL 及阶段间 WAL 变化；合成源文件与隔离目标 SQLite 哨兵在检查前后保持内容不变。
- [x] 检查初始与最终文件覆盖的是全部声明项目；缺失时不继续声称表、文件完整。错误不包含内容或路径。
- [x] 定向和完整 Rust 回归、格式/凭据检查、证据与独立本地提交；ABA 替换、真实句柄和目标切库仍待后续处理。详见 `docs/evidence/merge/task12b3-integrated-preflight.md`。

## Task 12c1：库内来源身份及恢复失效

**Files:** 新 `src-tauri/src/learning/schema-v31.sql`、`source_profiles.rs`、`source_profiles_tests.rs`；更新 `learning/mod.rs`、`backup.rs` 的正常与中断恢复、现有 schema 版本断言。无来源文件选择、读取、UI、任务或 Tauri 命令。

**Interface:** 后端内部 `scope(state, expectedLibraryId)` 使用现有 `with_library_if_id` 与 `analysis_store::library_identity` 捕获库 UUID 和规范根摘要；`create(state, scope)`、`get(state, scope, profileId)` 在同一活动库锁内重新核对两者，来源 profile/epoch 由后端生成规范非 nil UUID。v31 只保存 profile ID、固定 `yingliu`、epoch、创建时间；不保存路径、清单提示、密钥或批准，不代表已选择或授权真实来源。`invalidate_epochs(db)` 用于两条恢复路径，使未来本机批准不能因同目录恢复而继承；目前没有 grant 读取器。

- [ ] RED→GREEN：v30→v31 单事务升级和重复打开；人为触发迁移失败时表与版本同时回滚，旧笔记和归档不变。
- [ ] 两个不同根持相同 marker UUID、切库后旧 scope、未知/非规范 profile ID、缺失表或异常数据均不允许跨库创建或读取。检查新表没有任何源路径或授权字段。
- [ ] 真正的隔离整库备份恢复和中断恢复分别保留 profile 身份、轮换 epoch；隔离库以外无写入。旧归档来源键保持原样，不根据它自动建立新 profile。
- [ ] 定向与完整 Rust 回归、构建、格式/凭据检查及本地提交。来源选择与身份绑定、来源映射、用户授权及恢复事务仍属后续步骤；不能把 inert profile 视为授权。

## 后续顺序（本计划不提前激活）

1. **12b 合成 SQLite 与文件只读核验：** 固定隔离样本，`READ_ONLY` + online backup 覆盖活动 WAL，显式字段、schema 8、integrity/FK、行与文件摘要独立验证；读前后对 DB/WAL 及文件做指纹比较，测试变化、缺失、链接与占用。合成根由测试代码固定，不能接收任意源根。真正用户可选根及 Windows 句柄竞争安全须另设可验证步骤。
2. **12c 来源绑定和预检：** 用户明确选定来源并建立 UUID；来源/目标库身份、结构化 sourceKey、完整冲突与未解析引用、原文和修订快照，预检不写目标业务数据。不读取用户数据作自动验收。
3. **12d 持久任务和恢复：** 使用当前可用的新 schema、既有任务中心和每项操作记录；重复执行、文件暂存发布、取消、中断、原件变化和目标修订冲突均可恢复，不覆盖正文。
4. **12e 界面：** 接入现有学习区与任务中心，明确展示范围、来源、费用边界、预检结果和失败恢复；不从清单中的提示路径推导用户授权。
5. **Task 13–14：** 归档复制、按批撤回和最终真实桌面/性能/备份验收；真实源迁移、安装及发布仍需单独授权。

后续各步开始前，根据最新代码补齐精确文件、接口和失败测试；任何阶段的绿色合成测试均不等同于真实迁入完成。
