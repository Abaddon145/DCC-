# Task 9.3b1：持久转换预览与提交内核

日期：2026-10-24。BASE `7ac1644`，分支 `codex/yingliu-learning-integration`。本记录是 Rust 内核检查点；命令、主界面预览、实际 AppState 最终门及真实源迁入尚未接通，不等于整项 Task 9 或双应用联调完成。

## 本阶段结果

- 预览绑定当前库 UUID、规范目录摘要、已登记原文及完整 SHA-256、明确课程映射、目标笔记修订和受控附件快照。相同来源材料和目标复用预览；源身份由归档 ID 保留，不以同名/同内容跨来源合并。
- New 预览不创建空笔记；Replace 仅显式使用既有 noteId/revision，保留其标题、笔记本、标签、收藏和日志日期。提交前再次核对目标修订，冲突保留原文、预览和用户后续正文，不自动覆盖或代为另存。
- 新增 schema v28 单表 `learning_note_previews` 及身份索引，持久化有界快照、ready/committed/cancelled 状态和回执；正文仍唯一写到原 knowledge_notes，并复用修订、搜索、反链、附件、包导出能力。六处既有测试仅更新当前版本号断言，保留内容与身份检查。
- 附件参数仅供受信 Rust 来源适配器使用，不反序列化为前端路径权限。副本在当前库 `learning-data/note-previews/<previewId>/`，单项 100 MiB、合计 1 GiB、最多 1000 项；临时文件 fsync、独占发布、完整摘要和安全路径检查。图片格式/尺寸检查与既有 knowledge_files 共用；原附件移动后可继续从已验证快照提交。
- 课程映射读取实际课程/媒体/主文件/时长/本机授权和完整原件摘要，提交再次读取并比较；只读租约维持到操作结束。测试使用合成文件验证身份与摘要，没有调用解码器或把它计为真实视频播放验收。
- IMMEDIATE 事务内再次读取状态，登记原文片段并 remap，占位附件复制为当前笔记的真实附件后走 save_in，正文与回执同事务提交。重复提交或重启后重读旧回执，不重复写入，用户后续编辑保留。
- 最终提交回调若在实际 commit 后报告错误，先读取持久回执，再决定是否清理文件；已落库附件不会因回执中断被删。确定回滚时只删除本次新发布文件，不递归删除既有目录；取消只清理本预览登记的副本，原始归档保持不变。
- 批量原文片段一次读父原件和验证 UTF-8，逐段校验字节边界/摘要，savepoint 保证整批原子性；原有单段接口复用此实现。600,000 字节原件、1000 段重用、无效边界和第二条 SQL 写入故障均已运行检查。
- 备份继续复用既有 learning-data 范围。恢复保留原字节与登记，并撤销 ready 转换预览，要求重新预览；源库预览保持原状态。v27→v28 幂等升级保留旧正文 JSON、修订、更新时间和原字节。

## RED → GREEN

1. 预览/提交桩 4 组全部失败；首轮实现 2 通过、2 失败，分别暴露 remap 的可变索引给 atom 插入 `content:null`，及 Windows 扩展规范路径与普通路径比较导致失败清理遗漏。改为 get_mut 和统一规范根后 4 组通过。
2. 回执中断与坏图片新增测试先失败；改为清理前读取实际回执、预览前复用图片校验后 6 组通过。
3. 篡改快照/附件摘要、SQL 发布失败、真实课程原件变化等新增检查通过；批量登记桩先失败，再实现一次读取、边界验证及 savepoint。笔记定向 37 项通过。
4. 备份恢复检查先因旧预览仍为 ready 失败；接通恢复撤销后，连同升级检查进入 39 项定向通过。中途只读文件存在性判定因 std::io::Error 不实现 PartialEq 编译失败，改用 matches，未放宽错误处理。
5. 完整回归后，仅加强既有测试中的批量第二条 SQL 故障与库位置复制检查，生产逻辑未变；最终 11 组内核测试通过。位置检查的测试夹具最初用 execute_batch 读取带结果的 PRAGMA 而失败，改为 query_row 并检查 checkpoint 完成，未修改业务 SQL。

## 验证记录

- `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml learning_note --lib -- --test-threads=1`：**39 通过**，3.67 秒，进程 32332 退出 0；`task9-preview-final-targeted.log`。
- 完整 Rust：`cargo test --offline --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`：**457 通过、0 失败、2 既有 ignored**，126.04 秒，main/doc tests 0，进程 27699 退出 0；`task9-preview-full-rust.log`。
- 完整前端：**380 项 / 73 文件通过**，60.88 秒；`npm run build` 退出 0，7.70 秒，既有大 chunk 提示保留，进程 41671 退出 0；`task9-preview-full-ui.log` / `task9-preview-build.log`。
- 加强测试后最终 `learning_note_preview`：**11 通过**，0.96 秒，进程 9187 退出 0；`task9-preview-final-check.log`。SQL 整批回滚单项另运行通过，0.14 秒，`task9-preview-batch-final.log`。
- fmt/check、diff/check 均通过；凭据扫描排除无关 `Luca/`、`outputs/`、`tmp/`，未发现敏感信息。日志保存在忽略的 release-artifacts/yingliu-merge。

## 继续保留的边界

最终门目前用可控事务回调验证拒绝与回执中断；生产调用必须在 AppState 的短锁中检查库、位置与维护状态，再提交后台连接事务。该命令适配及可取消 IO 上下文将在 9.3b2 完成，不能先把接受磁盘路径的入口开放给前端。大量附件复制复用原有每笔记事务，已在代码记录写入竞争的已知上限；有真实竞争证据后再拆分文件发布，暂不新建队列。

还需检查并发重复请求、注册附件原文件名、受控预览图片读取、历史原件浏览/导出与计数，以及 Task12 的源快照、来源映射和后台任务接入。来源附件的未知文件或损坏快照不会被自动覆盖认领，突然中断留下的未登记文件由后续恢复报告处理。

没有使用真实用户数据、源 Vault、映流数据库或付费 AI；没有下载模型、推送、合并、发布或覆盖安装。事实核查子体使用 GPT-6 Luna medium；没有使用 GPT-6 Astra。物理 DPI/跨屏、真实 AppHandle/mpv、原用户事件和真实迁入仍按主计划分别验收。
