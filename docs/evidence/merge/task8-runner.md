# Task8.4检查点：课程分段后台任务

2026-10-24；基线0e187c7。本检查点只交付分段执行、部分失败续作和任务确认保护；不是整个Task8或8.4汇总阶段完成。

## 实现

- CourseAnalysisRequest使用courseId、previewId、typed stage；接入现有TaskRequest、submit、dispatch和后台spawn_blocking。必须明确expectedLibraryId；同库同课程同批准的活动任务去重，普通重试入口拒绝重放旧批准。
- run_segments复用全局ai::acquire_request、课程用途共享月度预算与原有HTTP实现。正式路径固定已有服务地址和凭据读取；测试传输入口只在cfg(test)编译且只允许127.0.0.1，无前端模型网址或密钥参数。
- 开始前验源、批准与全量费用预留；超额/预先取消不消费批准。真正开始时事务登记批准、分析和阶段；缓存批量复制采用原有worker connection，避免长事务持有AppState的当前库mutex。TaskRegistry的活动任务检查继续排斥正常切库/维护，不创建另一套队列或锁。
- 每个未缓存阶段发送前、响应后重新验源；提交期间持有新核验的原件只读lease。80ms监测取消/库变化，已发送但费用未知的请求保守保留预留。响应无法校验也不假定免费。
- 成功段逐项事务保存，后续失败不回滚前段；新预览只请求缺失阶段。记录明确的pending/completed/failed、实际tokens及分析partial/failed/cancelled状态。输出编码再守卫1MiB，不能写成随后无法读取的成功缓存。
- 完全命中缓存的新预览tokensMax为0；后台可以在测试服务已关闭时完成，不额外联网。汇总当前明确拒绝，不能把分段批准用来支付尚未预览的汇总请求。
- TaskCenter明确显示课程任务需重新预览确认，禁用通用重试并给出可见说明；保留全部活动/失败/中断/需操作任务，完成类历史限200条，列表摘要不带完整结果。继续沿用现有原生progress；只有真实阶段总量才显示计数。

## RED→GREEN与回归

日志均位于release-artifacts/yingliu-merge，使用隔离合成视频登记、真实SRT、本地HTTP、真实预算数据库和真实TaskRegistry日志，不读取用户凭据、不访问付费服务。

| 保护 | 证据 |
| --- | --- |
| 第四段失败保留前三段，新批准只续作第四段 | task8-runner-red.log的0!=3失败→task8-runner-green.log通过；HTTP实收时间为0/300000/600000/900000/900000ms，总计5次；已知用量404→505，坏结果计入费用 |
| 取消响应不能提交 | task8-runner-safety-red.log复现取消后仍保存1段→safety-green.log全部3项通过；修复执行器对已停止queued状态的判断，没有删除取消保护 |
| 来源变化/强制陈旧库上下文不能保存成功响应 | 安全测试在HTTP响应前更改原件或测试专用活动marker；0个成功阶段；已知来源变更请求的101用量保留。marker变更是故障注入，不声称用户可以绕过公开切库门禁 |
| 超额/预先取消 | 不到达传输层，批准仍未消费、分析表无半成品记录、课程用量0 |
| 队列去重、禁止通用重试 | task8-task-queue-red.log复现重复任务ID；去重后task8-task-retry-red.log复现错误放行；task8-task-queue-green.log通过 |
| 旧失败任务不被205个完成任务挤掉 | task8-task-history-red.log→history-green.log；完成历史仍仅200、摘要result为null |
| UI不重放付费批准 | task8-task-ui-red.log失败→ui-green.log10项通过，可见原因/原生disabled/aria-describedby；并未新增视觉框架或依赖 |
| 输出编码上限与原子性 | task8-stage-size-red.log在无大小保护时失败→size-green.log通过，超限阶段保持pending。最初一次链接失败因全量测试正在占用Windows测试exe，不算业务RED；退出后重新得到上述业务失败 |

最终验证：

- task8-runner-final-rust.log：`cargo test --locked --manifest-path src-tauri/Cargo.toml`，391通过、0失败、2项既有忽略，72.42秒。
- task8-runner-full-ui.log：完整前端70文件345项通过。
- task8-runner-final-build.log：生产构建退出0，既有大chunk提示保留；Rust仍有既有链接器消息，不伪称所有警告消失。
- fmt、diff检查退出0。提交前秘密扫描405文件、0疑似密钥，明确排除无关Luca目录。

## 边界与裁定

1. 按已批准顺序先保存可测试的分段检查点，汇总继续独立实现。未确认的汇总不能自动消费分段预算；代价是当前没有全片汇总入口，不能把此检查点标为8.4全部完成。
2. 完成/取消等任务历史不重放旧批准，继续须新预览；失败已知费用不因解析失败退款。代价是新预览明确展示新请求预算，而非无提示重试。
3. 分段缓存复制仍是同一SQLite原子事务，但不持有全应用当前库mutex；正常切库/维护由既有活动任务门禁排斥，重发前和响应后验源不可省。大课程、并发维护/物理中断性能验收仍未完成，归后续实测门禁，不以小fixture宣称通过。
4. 单元测试直接驱动生产课程runner，实际使用TaskRegistry.submit和本地HTTP；AppHandle/dispatch适配经过编译，但本检查点没有运行桌面真实课程任务，也没有双应用联调，模拟通过不等于这些验收通过。
5. UI技能仅用于失效动作的明确禁用与可见原因，沿用现有主题/组件；没有整体改版。完整分析面板仍在8.5，不自动写入笔记正文。

未修改映流代码、原始方案、用户媒体原件或Vault；无新库schema升级、付费请求、GitHub推送/合并/发布或用户安装覆盖。
