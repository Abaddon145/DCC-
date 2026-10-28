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

**Files:** 新 `src-tauri/src/learning/schema-v31.sql`、`source_profiles.rs`、`source_profiles_tests.rs`；更新 `learning/mod.rs`、`backup.rs` 的正常与中断恢复、`db.rs` 的旧版迁移及现有 schema 版本断言。无来源文件选择、读取、UI、任务或 Tauri 命令。

**Interface:** 后端内部 `scope(state, expectedLibraryId)` 使用现有 `with_library_if_id` 与 `analysis_store::library_identity` 捕获库 UUID 和规范根摘要；`create(state, scope)`、`get(state, scope, profileId)` 在同一活动库锁内重新核对两者，来源 profile/epoch 由后端生成规范非 nil UUID。v31 只保存 profile ID、固定 `yingliu`、epoch、创建时间；不保存路径、清单提示、密钥或批准，不代表已选择或授权真实来源。`invalidate_epochs(db)` 用于两条恢复路径，使未来本机批准不能因同目录恢复而继承；目前没有 grant 读取器。

- [x] RED→GREEN：v30→v31 单事务升级和重复打开；人为触发迁移失败时表与版本同时回滚，旧笔记和归档不变。修正旧迁移降低 schema 版本的问题，验证重新打开不会重建已解除的项目笔记关联。
- [x] 两个不同根持相同 marker UUID、切库后旧 scope、未知/非规范 profile ID、缺失表或异常数据均不允许跨库创建或读取。检查新表没有任何源路径或授权字段。
- [x] 真正的隔离整库备份恢复和中断恢复分别保留 profile 身份、轮换 epoch；隔离库以外无写入。旧归档来源键保持原样，不根据它自动建立新 profile。
- [x] 定向与完整 Rust 回归、构建、格式/凭据检查及本地提交。来源选择与身份绑定、来源映射、用户授权及恢复事务仍属后续步骤；不能把 inert profile 视为授权。见 `docs/evidence/merge/task12c1-source-profiles.md`。

**交付前置风险：** 12c1 当时的软件版本号尚未改变，原自动升级备份按应用版本触发且失败只提示。12c1b 增加了按数据库版本触发的本端保护；旧程序对 v31 的兼容性、实际安装及真实素材库恢复仍须单独验证。

## Task 12c1b：数据库升级前保护

**原因：** 12c1 核对发现，现有升级快照按应用版本去重且失败只警告；本分支应用版本仍为 0.23.0，无法确保旧 schema 升级前生成新快照。

**Interface:** 当前库已有数据库时，以只读方式读取实际 schema，允许读取已提交的 WAL；旧 schema 每次升级前生成并验证数据库快照，即使应用版本相同也不跳过。只有快照和其设置记录均成功后才打开数据库升级；失败阻止本次升级。已是当前 schema 的库维持原有备份警告行为，新库不生成升级前快照。中断恢复的升级路径同样传播保护失败并保留恢复标记；临时解包库不重复生成快照。当前程序在数据库写入前拒绝比自己更新的 schema。

- [x] 先测同软件版本旧 schema 不跳过、备份或设置记录失败不升级、新库及当前 schema 行为不变，确认 RED。保护或迁移失败时保留所选素材库，启动失败显示原因。
- [x] 只读检查不忽略 WAL，隔离测试验证仅存在 WAL 的已提交数据进入快照；不能把 DB/WAL 的文件副本当一致性快照。
- [x] 中断恢复失败后保留标记与原库，修复问题后可重试；强制保护未持久登记时不清理旧快照。未来或损坏的 schema 被拒绝；隔离测试中数据库与 WAL 字节保持不变。
- [x] 完整 Rust 回归、构建、格式/凭据检查、证据及本地提交。数据库快照不含媒体，不能直接当作整库备份，旧安装程序对新 schema 仍可能不兼容；详见 `docs/evidence/merge/task12c1b-upgrade-protection.md`。

## Task 12c2：笔记来源键与只读冲突判断

**Files:** 新 `src-tauri/src/learning/schema-v32.sql`、`note_origins.rs`、`note_origins_tests.rs`；更新 `learning/mod.rs`、`source_profiles.rs` 和相关升级夹具。没有来源文件选择、真实来源读取、迁入任务、UI 或 Tauri 命令。

**Interface:** 暂只处理来源 Markdown 与目标笔记。结构化键严格保留 `[profileId, "markdown", oldVaultId, relativePath]` 四个字符串，规范 JSON 编码、不拼接、不截断；来源身份必须是本库存在的规范 profile。v32 保存来源键、原文摘要、目标笔记 ID、上次成功后的笔记修订和笔记快照摘要，不保存源根、文件权限或正文。登记只允许在调用方已有的数据库事务中执行，且目标笔记实际存在；重复登记必须完全一致，不允许替换旧映射或把不同 Markdown 身份自动合为同一笔记。范围检查核对素材库身份和来源 epoch；只读检查分开报告未映射、未变、来源变化、目标变化、双方变化、目标不可用；这些状态不构成真实文件核验或自动更新许可。其他课程、系列、附件和历史对象的映射留待对应实现，不臆造其目标身份。

- [x] RED→GREEN：键中的斜杠、Unicode 和分隔符不碰撞；错 profile、无效 UUID、空值、控制字符及超长拒绝，长键原样保存不塞入旧归档键。
- [x] 在隔离数据库中验证 v31→v32、重复打开及失败回滚；登记和笔记创建共享事务，失败不留半份映射或笔记；重复登记幂等而变更或不存在的目标拒绝。
- [x] 目标内容或导航颜色变化、来源摘要变化、两者都变、目标进入回收站/删除各自报告；相同名称或内容不能跨来源自动合并。检查不修改业务数据。
- [x] 备份/恢复保留历史映射与目标笔记，同时仍轮换来源 epoch；完整 Rust 回归、构建、格式/凭据检查、证据及本地提交。见 `docs/evidence/merge/task12c2-note-origins.md`。

**边界：** 笔记快照覆盖现有笔记记录，不证明外部附件文件、关联对象或源文件没有变化；后续应用阶段必须重新校验全部将被修改的对象与文件。活动 SQLite 的 DB/WAL/SHM 实际打开对象、目录替换竞争以及独立 Vault 授权仍未实现，不能以测试夹具代替真实来源读取。

## Task 12c2a：切库后的来源范围失效

**原因：** 当前范围只包含库 UUID 和目录摘要，A→B→A 后旧范围会重新匹配；未来待确认来源不能沿用该状态。

**Interface:** 每个 `AppState` 持有随机的内存会话标识，成功切库时在活动库锁内轮换；`source_profiles::scope/scoped` 在同一锁内捕获和复核。失败切库不轮换；重建 `AppState` 同样不能复用旧范围。该标识不持久化，也不替代恢复时的 profile epoch 或任何源文件授权。

- [x] RED→GREEN：切 A→B→A 后旧范围拒绝，重新获取范围可读已有来源身份；不同状态实例指向同一库时旧范围也拒绝。
- [x] 切库失败后原范围仍可用；并发检查与切库按锁顺序串行，记录或检查不访问源文件。定向与完整 Rust 回归、格式/凭据检查、证据及独立提交。见 `docs/evidence/merge/task12c2a-scope-session.md`。

## Task 12c2b：笔记附件与引用变更保护

**原因：** 添加笔记附件不会增加正文修订号；单靠 `Note` 快照可能把已改变的目标标为未变。

**Interface:** 将笔记记录、附件登记元数据和显式引用按稳定顺序计算目标快照；范围内的检查在同一数据库读取事务中进行。只比较数据库登记信息，不声称已核验附件文件内容。旧格式的快照摘要若不匹配，保守报告目标已变化，不自动改写历史记录。

- [x] RED→GREEN：通过真实笔记附件导入流程增加附件后，正文修订未变但来源检查报告目标变化；显式引用变动同样检测到，不修改用户笔记。
- [x] 验证重复检查稳定、元数据读取失败拒绝；保持原有正文、颜色、恢复和切库测试。定向与完整 Rust 回归、格式/凭据检查及证据见 `docs/evidence/merge/task12c2b-note-metadata.md`。

## Task 12c3a：Windows 活动 SQLite 文件保护实验

**Files:** 仅扩展 `learning/import_snapshot_tests.rs` 的 Windows 条件测试；复用已有合成 SQLite 快照测试。没有生产来源访问入口、用户路径、迁入或授权。

**范围：** 在临时目录中，对目录及已存在的合成 DB/WAL/SHM 使用允许读写但不共享删除的文件句柄，验证同进程内替换被阻止、SQLite 已提交写入和只读快照仍可进行；另验证不存在的侧文件无法被这种句柄预先保护。只报告实验结果，不将句柄实验视为 SQLite VFS 实际打开对象已绑定。

- [x] 验证被保护的目录、DB 和已存在侧文件不能被重命名替换，同时活动写连接及包含 WAL 的快照继续工作。
- [x] 验证缺失侧文件仍可被创建，避免把对已有文件的保护误称为完整路径安全。
- [x] 记录跨进程、目录祖先及重解析竞争的未验证范围；定向测试、格式和凭据检查后单独提交。详见 `docs/evidence/merge/task12c3a-windows-leases.md`。

## Task 12c3b：跨进程的 Windows 文件保护实验

**范围：** 继续只使用临时合成目录和测试进程。父进程持有目录、DB/WAL/SHM 句柄；子进程尝试重命名这些对象，并通过独立 SQLite 连接提交数据；父进程验证只读快照包含子进程提交。子进程有超时和清理，不连接真实映流进程。

- [x] 验证跨进程替换被拒绝、独立写连接仍可提交、快照读取已提交数据。
- [x] 如实记录侧文件缺失、路径祖先、重解析、VFS 对象身份和源程序 checkpoint 等未覆盖范围；完成定向测试、格式/凭据检查和独立提交。见 `docs/evidence/merge/task12c3b-cross-process.md`。

## Task 12c4：独立 Markdown 来源的内部检查

**范围：** Windows 内部接口只检查调用方明确选择的 Vault 和单篇 Markdown，不接入 UI、文件授权或导入；不打开映流数据库。锁定来源路径各目录及文件，拒绝重解析点、硬链接、越界和过大文件，读取实际字节并复核当前库及 profile epoch，再用已有来源映射报告冲突。所有测试仅使用临时目录。调用方仍须建立独立授权；检查结果不能替代实际写入前再次验证。

- [x] RED→GREEN：临时 Vault 的真实文件及目标映射，覆盖未映射、未变、来源变化、目标变化与路径拒绝。
- [x] 验证切库、epoch 失效、文件写入互斥和内容大小限制；相关回归及局限见 `docs/evidence/merge/task12c4-markdown-source.md`。

## Task 12c5：临时 Vault 选择范围

**范围：** 内部临时选择与当前库范围、profile、epoch、Vault 身份及选定目录绑定，持有目录链句柄；同一选择多次检查只能访问该目录，切库或恢复后拒绝。检查完成后不持久化来源权限，不提供 Tauri 命令或迁入入口。选择不是用户授权机制，生产选择和恢复流程尚须实现。

- [x] RED→GREEN：临时目录选择后无法被重命名替换；多篇检查复用相同范围，不允许在检查时传入其他根目录，切库及 epoch 更新后拒绝。
- [x] 定向及完整回归、生产构建、格式与凭据检查；目录权限及验证边界见 `docs/evidence/merge/task12c5-vault-selection.md`。

## Task 12c6：单篇笔记的只读转换预检

**范围：** 对被临时选择的单篇 Markdown，复用文件保护与现有转换器，从同一份读取字节生成冲突状态、摘要和转换结果；未绑定的媒体和附件明确待处理，不写目标笔记或归档。检查完成前再次验证目标库范围。此阶段不提供 UI、持久来源授权或真实迁入入口。

- [x] RED→GREEN：真实临时 Markdown 的转换内容与来源摘要一致；附件未解析和无效 UTF-8 保守待处理，原文件与目标库未修改。
- [x] 定向和完整测试、构建及凭据检查，见 `docs/evidence/merge/task12c6-note-preflight.md`。

## Task 12c7：来源 Vault 身份登记

**范围：** schema v33 新建来源 profile + Vault 身份绑定；仅保存锁定目录句柄所得的本机卷与文件标识，不存源路径和访问授权。首次登记须在用户明确选择后的事务中完成；已有绑定只能匹配复用，旧 profile 已有笔记映射但没有 Vault 绑定时拒绝自动绑定。恢复仍轮换 epoch，身份不是读取授权或跨机器稳定标识。

- [x] RED→GREEN：空库首次登记、相同身份重试、不同目录拒绝、旧映射不自动绑定；v32→v33、失败回滚和备份恢复保留映射。
- [x] 保留旧素材与功能，完整回归、构建、凭据检查；真实目录选择 UI 和跨设备重定位仍待实现。详见 `docs/evidence/merge/task12c7-vault-identity.md`。

## Task 12c8：预检强制核对目录身份

**范围：** 每次 Markdown 预检在读取前检查 profile、epoch 和已登记的真实目录身份；转换后与目标冲突检查在同一只读事务中再次检查，未登记、身份不符或记录损坏均拒绝。仍由调用方取得独立来源授权，不实现路径选择 UI 或迁入。

- [x] RED→GREEN：未登记、另一个目录、登记删除或篡改不能预检；相同目录有效，切库和恢复后旧选择失效。
- [x] 完整回归、构建和证据；真实来源授权及新 profile 与首个绑定的原子创建仍待实现。详见 `docs/evidence/merge/task12c8-identity-enforcement.md`。

## Task 12c9：原子创建或复用来源

**范围：** 调用方完成独立用户授权后，内部操作持有选定 Vault 目录句柄，在同一目标库事务中创建来源 profile 与目录身份绑定；同 Vault 和物理目录的再次选择复用已有 profile。切库、中断或数据库错误不留孤立 profile；模糊或损坏记录拒绝。仍没有用户目录选择 UI、实际来源授权或迁入入口。

- [x] RED→GREEN：首次创建、重复选择复用、事务故障回滚、旧范围失效；既有来源和用户文件不改变。
- [x] 测试备份恢复后重新选择及全量回归；身份并非权限或跨设备稳定凭证。详见 `docs/evidence/merge/task12c9-atomic-source.md`。

## Task 12c10：Windows SQLite 实际文件句柄核验

**范围：** 仅使用隔离的合成 Windows SQLite WAL 数据库，验证 SQLite 官方 file-control 接口可否取得实际主库和 WAL 的底层句柄，并与持有的文件句柄按卷序列号和文件索引核对。该实验不接入用户路径或产品读取器，不证明 SHM、缺失侧文件、完整路径授权或跨进程竞争安全。

- [x] 测试主库和 WAL 的 SQLite 句柄确实对应锁定的合成文件，错误文件身份被拒绝，活动写入和 WAL 快照可用。
- [x] 定向测试、格式及凭据检查，具体限制见 `docs/evidence/merge/task12c10-sqlite-handles.md`。

## Task 12c11：齐全 sidecar 的隔离快照验证

**范围：** 仅在临时合成目录中，先对已存在的主库、WAL、SHM 和目录持有 Windows 句柄，不存在任一文件时拒绝而不创建；用同一个 SQLite 只读连接核对实际主库/WAL 句柄后创建在线快照。实验不提供产品路径读取或用户授权，不宣称 SHM 对象身份和全部重解析竞态已获证明。

- [x] RED→GREEN：sidecar 齐全时可获取包含 WAL 的快照且写入继续；缺失时不打开 SQLite、不创建文件；路径替换失败。
- [x] 定向测试、格式及凭据检查，测试环境与剩余边界见 `docs/evidence/merge/task12c11-sidecar-snapshot.md`。


## Task 12c12：关闭写入端后的 SHM 只读控制

**范围：** 仅在临时合成 Windows WAL 库上，验证写入端退出后普通只读打开仍会改写 `-shm`，并用 SHM 的 `FILE_SHARE_READ` deny-write 租约在 snapshot 路径上阻止该写入，同时保持 online Backup 可用。不接入用户路径或产品读取器；活动写入端已持有可写 SHM 时租约回退，该竞争未关闭。

- [x] RED→GREEN：默认只读打开改写关闭写入端后的 SHM；deny-write 租约路径快照保留 SHM/WAL/DB 且读到 WAL 行；独立 SHARE_READ 租约同样挡住读者写 SHM。
- [x] 记录 `readonly_shm=1` 与 Backup 不兼容、活动写入端残留；定向测试、格式及凭据检查，见 `docs/evidence/merge/task12c12-shm-readonly-control.md`。


## Task 12c13：活动写入端下的 SHM 失败闭合

**范围：** 仅在临时合成 Windows WAL 库上，去掉 12c12 在 deny-write SHM 租约失败时的静默普通只读回退；租约失败则拒绝打开。不接入用户路径或产品读取器；不把显式非安全并发读取路径当作生产授权。

- [x] RED→GREEN：活动写入端占用可写 SHM 时 `snapshot_synthetic` 失败且 sidecar 字节不变；写入端退出后 deny-write 路径仍可快照；并发 WAL 可见性仅保留在显式非安全测试中。
- [x] 定向测试、格式及凭据检查，见 `docs/evidence/merge/task12c13-active-writer-shm-fail-closed.md`。


## Task 12c14：活动写入端下的完整 sidecar 拷出快照

**范围：** 仅在临时合成 Windows WAL 库上，在活动写入端占用可写 SHM 时，用共享读租约拷出齐全 DB/WAL/SHM，再对副本做 deny-write SHM 快照。不打开源 SQLite，不接入用户路径或产品读取器；不宣称撕裂拷贝或源上并发 RO 已解决。

- [x] RED→GREEN：源上 `snapshot_synthetic` 仍失败闭合；拷出路径读到已提交 WAL 且源 sidecar 字节不变；缺失 sidecar 拒绝且不创建。
- [x] 定向测试、格式及凭据检查，见 `docs/evidence/merge/task12c14-sidecar-copy-out.md`。

## Task 12c15：拷出路径撕裂检测与静默点重试

**范围：** 仅在临时合成 Windows WAL 库上，为 12c14 拷出路径增加拷贝前后 SHA-256+长度指纹比对、失败闭合清理与最多 3 次重试；测试钩子注入确定性中途变更。不打开源 SQLite 作产品读取，不接入用户路径；不宣称源上实时并发 RO 已解决。

- [x] RED→GREEN：中途 SHM 变更拒绝且不留下不完整副本；稳定指纹集接受；定向测试见下方凭据。
- [x] 定向测试、格式及凭据检查，见 `docs/evidence/merge/task12c15-torn-copy-detect.md`。
## Task 12d1：持久迁入批次与条目恢复底稿

**Files:** 新 `src-tauri/src/learning/schema-v34.sql`、`import_ops.rs`、`import_ops_tests.rs`；更新 `learning/mod.rs`（schema 33→34）、`source_profiles.rs`（scope 只读访问器）；同步现有“当前 schema=33”断言到 34。无 Tauri 命令、无 UI、无真实映流路径读取；调用方必须已提供核验过的字节与摘要。

**Interface（内部 `pub(super)`）：**
- `create_batch(state, scope, profile_id, epoch, request_sha256) -> Batch`：校验 scope/epoch；同库同请求摘要的 pending/committed 批次幂等返回；不授权来源读取。
- `add_item(state, scope, batch_id, NewItem{source_key, source_sha256, title, document_json, expected_target?}) -> Item`：仅 pending 批次；`source_key` 为 Markdown 结构化键；同批次同键拒绝冲突登记。
- `stage_item_bytes(state, scope, batch_id, item_id, bytes)`：写入库根下 `.dcc/runtime/imports/<batch>/<item>.md`（原子写入），核对 SHA-256 与声明一致后标 `staged`；超限/摘要不符失败闭合且不留半份成功登记。
- `commit_batch(state, scope, batch_id)`：逐项在同一库事务外按项处理——`Unmapped` 用 `knowledge::save_in` 新建笔记并 `record_note_in`；`Unchanged` 不改正文仅标已提交；`SourceChanged`/`TargetChanged`/`BothChanged`/`TargetUnavailable` 标 `conflict` **绝不覆盖正文**；已提交项删除对应暂存。批次写入 receipt（计数），状态 `committed`（可含冲突项）。
- `cancel_batch(state, scope, batch_id)`：pending/staged/conflict 项改为 cancelled，删除暂存文件，批次 `cancelled`。
- `list_batches(state, scope) -> Vec<Batch>`：只读列出本库批次（含中断遗留的 pending）。

**范围边界：** 不接入任务中心 UI（12d2/12e）；不打开真实用户 Vault/SQLite；不把清单路径当授权；暂存属 runtime 可清理，不是原文归档。

- [x] RED→GREEN：v33→v34 升级与失败回滚；幂等批次；暂存/取消清理；新建提交登记来源；目标修订冲突不覆盖正文；中断后 pending 可列表并取消恢复。
- [x] 定向测试、格式及凭据检查与证据；独立本地提交。详见 `docs/evidence/merge/task12d1-import-ops.md`。

## 后续顺序（本计划不提前激活）

1. **12b 合成 SQLite 与文件只读核验：** 固定隔离样本，`READ_ONLY` + online backup 覆盖活动 WAL，显式字段、schema 8、integrity/FK、行与文件摘要独立验证；读前后对 DB/WAL 及文件做指纹比较，测试变化、缺失、链接与占用。合成根由测试代码固定，不能接收任意源根。真正用户可选根及 Windows 句柄竞争安全须另设可验证步骤。
2. **12c 来源绑定和预检：** 用户明确选定来源并建立 UUID；来源/目标库身份、结构化 sourceKey、完整冲突与未解析引用、原文和修订快照，预检不写目标业务数据。不读取用户数据作自动验收。
3. **12d 持久任务和恢复：** 使用当前可用的新 schema、既有任务中心和每项操作记录；重复执行、文件暂存发布、取消、中断、原件变化和目标修订冲突均可恢复，不覆盖正文。
4. **12e 界面：** 接入现有学习区与任务中心，明确展示范围、来源、费用边界、预检结果和失败恢复；不从清单中的提示路径推导用户授权。
5. **Task 13–14：** 归档复制、按批撤回和最终真实桌面/性能/备份验收；真实源迁移、安装及发布仍需单独授权。

后续各步开始前，根据最新代码补齐精确文件、接口和失败测试；任何阶段的绿色合成测试均不等同于真实迁入完成。
