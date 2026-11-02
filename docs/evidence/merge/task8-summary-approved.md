# Task8.4：独立确认的课程汇总任务

2026-10-24；基线a0f8e5f。沿用schema26、TaskRegistry、现有月度预算及不可变快照，不新增队列、数据库服务或依赖。此记录是后端与本机模拟服务验证，不是原生桌面/付费服务验收。

## 实现

- `prepare_course_summary(expectedLibraryId,courseId,parentAnalysisId)`只读取本库完整、已核验的父结果，重新核对原件只读lease、完整hash、字幕和库身份，再登记24小时有效的独立预览。前端不提交路径、结果、指纹、层级或请求计数。
- 后端快照增加可选summary上下文：父分析UUID、父指纹、累计逻辑阶段数与完整分组。无summary时不序列化该字段，旧分段快照字节与旧指纹公式不变；沿用16MiB快照上限。没有增加数据库列或修改映流。
- 每层从实际完整父结果推导层级、计数和请求体；最长8层、逻辑汇总阶段容量64。新层必须重新预览确认；不预先批准未知输出。计数和谱系不能由重新计算本地hash伪造。
- 原件及完整祖先结果在请求前后复核；开始分析时，父结果检查、批准消费、分析/阶段创建在同一事务。父结果变化时事务拒绝，不消耗原批准、不留下半份记录。历史仍可查看，但标为来源陈旧，不能当作新批准。
- 分段与汇总共享runner、单请求AI锁、取消监控、任务日志和真实用量结算；类型不匹配在消费批准前拒绝。汇总不再次上传JPEG，实际请求预算必须等于已批准预算。
- 成功组独立保存；失败后新预览复用成功组，仅申请缺失组费用。完整缓存可在模拟服务已关闭时完成，费用不增长。最终单组才发布为整体result，仍有pending/failed阶段时不能标记completed。中间多组不会伪装成最终摘要。
- 已注册类型化后端命令与前端API；分析面板、桌面实际调度和交互验收仍属8.5，不能称整个Task8完成。没有写笔记或触发新安装。

## RED→GREEN 与本地证据

日志在忽略目录`release-artifacts/yingliu-merge/`：

- `task8-summary-approval-red-native.log`：有效父结果在未实现stub处失败；后续通过单次批准、错库/过期沿用校验、父内容改变后不消费批准、旧快照不新增null字段。最初沙箱临时文件权限失败单列在red.log，不当作业务RED。
- `task8-summary-live-red.log`：实时来源包装未实现；后续`task8-summary-live-green.log`六项通过，含3+1分组、缓存复用、第二层合并、祖先内容变化后旧批准失效且历史保留。中途失败来自测试复用了较早now，已改用各预览实际createdAt，未放宽生产时间检查。
- `task8-summary-runner-red.log`：错用分段请求消费了汇总批准；`task8-summary-queue-red.log`继续定位汇总队列拒绝；`task8-summary-terminal-result-red.log`复现最终结果未发布；`task8-summary-pending-red.log`复现pending被提升completed。均由生产共享路径修正。
- `task8-summary-runner-green.log`：课程分析定向30项通过。真实TaskRegistry持久任务与localhost HTTP驱动同一生产runner，不替换预算/存储。两组汇总第二组失败、重新确认只重发第二组、下一层最终合并：HTTP起点`[0,900000,900000,0]`，累计实际用量404；再读完整缓存0新请求。
- `task8-summary-safety-green.log`：四项runner测试通过，原分段与汇总均覆盖取消、原件替换、强制测试库标识变化、额度不足/预取消；汇总另测祖先结果损坏。已知费用保留101，未知取消预留不冒称免费；无无效结果写入。
- `task8-summary-lineage-green.log`：构造真实持久父链到第8层，累计阶段数由0到28，最后一层只剩最终单组；伪造前序计数被拒绝且批准未消费，不能请求第9层。

## 完整回归

- `task8-summary-approved-full-rust.log`：`cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`退出0；399通过、0失败、2项既有忽略，116.18秒；main与doc tests各0项。两项忽略为20,000笔记生成性能基准、需真实Markdown输入的网络图片人工验收，未伪称已跑。
- `task8-summary-approved-full-ui.log`：70文件、345项通过，61.53秒。
- `task8-summary-approved-build.log`：生产构建退出0；既有大chunk警告保留。Rust既有Windows链接/增量缓存警告未隐藏，不作为业务失败。
- 格式及diff检查退出0；提交前秘密扫描409文件、0疑似密钥，明确排除无关Luca目录。

## 边界与裁定

- 64为单条父结果谱系中的逻辑汇总阶段容量；失败重试必须逐次新确认，按实际请求继续结算共享月度额度，不是免计费，也不是用户终身调用次数上限。
- 可选summary上下文复用既有快照，未新增第二套批准表；原始cue/frame仍是时间及来源验证依据。代价是多层汇总需要读取有界祖先快照，大材料性能仍需最终验收，不宣称已完成长时/断电测试。
- 不自动开始下一层、不自动修复坏响应、不自动写知识库正文；真实桌面、DPI/跨屏和长时验收待8.5/Task14。没有付费调用、推送、合并、发布、覆盖安装、修改源仓库或用户原件。无关Luca目录未读取或暂存。
