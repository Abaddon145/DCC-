# Task 9.2：学习引用与保留原文节点

日期：2026-10-24。BASE `aee7b74`，分支 `codex/yingliu-learning-integration`。本记录覆盖 typed 节点及生命周期；原文转换预览/提交（9.3）、历史迁入、真实 Windows 用户验收尚未完成，不能据此标记整项 Task 9 完成。

## 当前可验证能力

- `learningReference` 与 `preservedMarkdown` 共用现有 `document_json`、保存版本、搜索索引、回收站、附件和编辑器。字段严格验证，额外 HTML/路径/子内容不被接受；时间引用使用课程 UUID、媒体 UUID、原件完整 SHA-256 和多个非连续范围。
- 保留原文只保存库内归档片段 UUID 与标签；保存/粘贴核对当前库登记。原文查看命令绑定 library/note/item，允许当前笔记及其真实历史，拒绝其他笔记借用标识。原文懒加载到只读 textarea，HTML 不执行，支持失败重试和丢弃旧响应。
- 编辑器、阅读、大纲与导图复用同一节点实现。共享事务保护覆盖删除/剪切/粘贴覆盖；移动不重复询问，删除可取消与撤销。内嵌 BlockEditor 的修改交给主正文确认一次，拒绝后两处视图同步恢复。
- 时间引用在现有反链表登记为媒体引用；删除来源不会伪造新来源。点击时后台验证当前库、课程与媒体关系、时长、授权及完整原件摘要。App 复用现有离开保存守卫，进入原生播放后首次 ready 执行暂停定位；后续手动 seek 不重置，返回明确的原笔记 ID。
- 原生播放启动支持可选 expectedFingerprint，在受控原件读租约下再次核对，堵住预检查与启动间替换原件的间隙。保留旧三参数 Rust QA 入口与正常播放器调用方式。
- 有保留块的笔记包使用 version2，旧 version1 保持兼容。包内仅携带被引用片段的原字节，目标库重新登记并 remap UUID。完整原始文档仍随整库备份保存，不会因复制一个块而夹带未引用段落。
- 显式 Markdown 导出恢复原始语法和换行，匹配既定占位后一次性插入，不递归解释原文里的占位。HTML 转义输出，TXT 保留正文顺序、原文及时间来源元数据；原文损坏时导出失败，不生成空白假成功。
- 无新依赖/数据库表/队列；schema 仍为 v27，不重写旧 JSON。没有来源 Vault 写回、自动 AI 或自动视频复制。

## RED → GREEN

1. 后端节点测试先因未知节点失败；接通校验、保存、索引与剪贴板后通过。未登记归档保存拒绝且 revision/history 不变化；同库复制后删除源笔记，目标原文仍可读。
2. 导出先因 HTML 无原文、包仍为 v1 失败；接通 v2 包、MD/HTML/TXT 后通过。另有五类恶意包（原件/片段摘要、范围、引用清单、路径标识）核验失败，目标笔记与归档均未发布。
3. 包片段保护测试先导出 `PRIVATE/PUBLIC/SECRET` 整篇，改为只导出 `PUBLIC` 后通过。此修正替代最初子计划的完整父文件包设计；代价是同一来源的多个片段独立占包文件条目，沿用 1000 文件/1GB 限制。
4. 前端空扩展先 5 项失败；真实 Tiptap 节点 6 项通过。中途修正共用时间函数的实际路径，属性不输出为散乱 HTML 属性；测试等待 React NodeView 挂载，并区分 StarterKit 的末尾空段落，不放宽节点数据断言。
5. 实际 KnowledgeWorkspace/MarkdownManager 集成先因未注册节点和空导出失败。随后发现 topicId/topicDepth/topicColor 被附带到接口；改为构造精确引用 DTO，保留后台严格解析。
6. App 时间引用、首次 ready 后定位均先缺入口失败，再接通。原生会话指纹测试先接受错误摘要而失败；启动源租约重新验证后通过。旧 QA 的七处调用在编译时暴露签名变化，保留兼容包装，没有修改 QA 语义。
7. 真实大纲内嵌编辑器先弹两次确认而失败；统一主正文确认，拒绝时内嵌同步恢复后通过。该路径与正文/撤销/拖动均有运行检查。

关键日志位于忽略的 `release-artifacts/yingliu-merge/`：`task9-nodes-red.log`、`task9-node-exports-red.log`、`task9-node-integration-red.log`、`task9-nodes-ui-red.log`、`task9-package-segment-red.log`、`task9-native-fingerprint-red.log`、`task9-outline-deletion-red.log` 与对应 green 日志。中途脚本断言因格式化后调用分成多行而拒绝改写，随后对两处真实调用精确修复；失败日志保留，未作为通过证据。

## 最终验证

- 完整 Rust：`cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`，**437 通过、0 失败、2 既有 ignored**，124.23 秒，main/doc tests 0，进程 49708 退出 0。日志 `task9-nodes-full-rust.log`。
- 完整前端：`npm test`，**376 项 / 73 文件通过**，45.65 秒。生产 `npm run build` 退出 0，4.38 秒，保留既有大 chunk 提示；进程 42134 退出 0。日志 `task9-nodes-full-ui.log` / `task9-nodes-build.log`。
- 定向核心：17 项 learning_note 测试在新增恶意包/原文占位用例前通过；新增用例均纳入上述最终完整 Rust。实际编辑器/播放器/App 定向 57 项通过；修复内嵌删除后两文件 33 项通过，并纳入最终完整前端。
- fmt/check、diff/check 均退出 0；最终凭据扫描 432 文件、0 命中，排除无关 `Luca/`、`outputs/`、`tmp/`。
- 完整回归后仅增强既有定向测试中的 typed 节点软删除/恢复断言，生产代码不变；`task9-node-recycle-final.log` 单项通过，确认文档、原文与搜索结果恢复一致。

## 保持待验收

两个既有 ignored 仍为大规模笔记 benchmark 与真实网络图片验收。本阶段运行的是隔离 SQLite/文件/真实 Tiptap 组件和原生会话源校验，不是实际 AppHandle/mpv 渲染、真实源迁入、Whisper 模型、付费 AI 或物理 DPI/跨屏验收。9.3 的源 Markdown 支持矩阵、转换预览与提交幂等、未转换历史浏览/计数仍需继续，源归档成功不等于正文转换成功。

包内原文发布先于笔记事务；若稍后的笔记提交失败，已核验原文可留作待处理，接口仍报告失败，重试复用摘要身份。突然断电留下的未登记 UUID 文件不被读取或自动认领，Task 12 恢复报告处理。没有自动推送、合并、发布或覆盖安装。
