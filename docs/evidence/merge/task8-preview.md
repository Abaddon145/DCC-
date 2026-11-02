# Task8.3：材料预览、快照与只读历史

2026-10-23；基线23d4c11。仅栈藏本地开发检查点，沿用v0.23后的资产核心/智能功能。映流仓库、原始方案、用户原件及Vault未改动。

## 本阶段实现

- schema26增量增加learning_analysis_previews、learning_analyses、learning_analysis_stages及查询索引，保留旧素材与学习身份。v25升级与两个连接同时确认的测试证明只有一次成功登记。
- prepare_course_analysis按expectedLibraryId与courseId读取现有媒体授权/只读lease、完整SRT/VTT及显式选择的真实帧。复用已有FFmpeg进程管理、JPEG转换与course_budget，无HTTP请求、无默默转写/下载模型。
- 快照包含库UUID、物理根目录hash、媒体文件ID/实际SHA-256、字幕ID/SHA、标题/时长、完整分段、帧/图像hash、模型及模板/提示/格式版本。只把UUID+SHA的安全相对路径登记在数据库；原子创建不可变文件，数据库登记失败清理本次新文件，不覆盖已有文件。
- 批准24小时失效、只消费一次；必须后端重新验源，不能由前端传PreparedCourseAnalysis或指纹代替确认。开始记录与所有阶段写入在同一事务。批准期间费用、缓存标记或任何快照字段变化均不能通过相同来源指纹绕过精确快照校验。
- 保存成功段只可在完整材料/模型/模板/提示/格式身份相同时复用；读取时重新进行结构与来源校验，保留并严格检查本机生成的历史纠正记录。批准的缓存后来失效则事务回滚，不能偷换成新增付费请求。
- get_course_analysis按库/课程/分析UUID读取只读历史。阶段元数据最多50条一页，只读显式选择的单个结果或汇总，每个JSON至多1MiB；坏结果不显示为有效。原件、字幕或访问批准改变会返回staleReasons，历史读取不会重新激活过期/已消费批准。
- 备份恢复及中断恢复撤销未消费批准，快照/历史仍随learning-data保留。不同物理位置复制来的批准不可用。

## 行为验证

全部材料为隔离测试库与合成视频/字幕，未调用外部AI或读取用户凭据。

| 行为 | 实际证据（release-artifacts/yingliu-merge下） |
| --- | --- |
| 存储、路径/身份、过期、一次消费 | task8-preview-store-red.log的stub失败→store-green.log通过 |
| 修改费用但保留来源指纹不可批准 | task8-preview-consent-red.log失败→consent-green.log通过 |
| 实际原件/字幕替换不消费批准 | task8-preview-source-red.log→source-green.log |
| 旧成功段与本机纠正诊断复用 | task8-preview-cache-red.log→cache-green.log |
| 恢复后不能沿用旧批准 | task8-preview-restore-red.log失败；后续全量通过 |
| 七个真实采样点分6+1请求，不是整课只许六帧 | task8-preview-multiframe-red.log明确因六帧全局上限失败→multiframe-green.log通过 |
| 已过期/消费的历史可读，跨库/错课程/错阶段拒绝、坏结果隐藏 | task8-preview-history-red.log→history-green.log |
| 真实采样稳定，未改原件hash/mtime，损坏视频不登记成功预览 | 完整定向回归及实际FFmpeg测试 |
| 16MiB超限不截断、不登记/发布半份快照 | oversized_snapshot行为测试 |

task8-preview-complete-targeted.log：19项通过，0失败，68.86秒，含已有预算localhost HTTP测试。第一次多帧测试沙箱拒绝临时文件原子保存；正常权限重跑后才记录真正业务RED，并非把环境失败作为行为RED。

task8-preview-final-ui.log：70文件344项通过。task8-preview-final-build.log：生产构建退出0，仍有既有大chunk提示。task8-preview-verified-rust.log：完整Rust385通过、0失败、2项既有忽略，72.69秒；fmt/diff检查退出0；秘密扫描402文件、0疑似密钥，明确排除无关Luca目录，不读取或修改它。

先前全量回归曾出现4项旧schema版本断言仍期望25、实际已26的失败，已按实际增量迁移修正backup/asset/intelligence/db测试期望；没有为通过测试撤回生产迁移或删除检查。最终全量回归为上述385通过。

## 裁定与边界

- 六帧是每次请求限制，整课可按真实边界拆段，最多4096段；未用全局六帧截断课程。不可变JSON快照整体16MiB上限，采样增量累计守卫内存，超限明确失败、不丢正文。若实际课程需要更大快照，应改为分文件流式图像，而非无界增加内存或默默缩小材料范围。
- 历史读取与付费批准读取共用身份/hash校验，但只有只读历史可忽略TTL/消费状态。没有把旧批准续期、没有修改已保存结果。
- TaskRegistry目前无通用本地预览读lease；本阶段复用原件读lease和现有维护状态检查，提交前在当前库锁内复核身份，提交前释放视频lease，避免持有原件锁等待恢复。付费前必须再次完整验源，不能以预览完成证明稍后原件仍有效。并发维护/物理中断验收仍列最终门禁。
- 本阶段只有prepare/get命令与类型接口。CourseAnalysis付费任务、分段部分失败续作、汇总、分析面板和真实双应用联调尚未完成，不声称整个Task8/M3或集成已完成。
- 不推送、不合并、不发布、不覆盖用户安装。既有2项忽略及物理DPI/跨屏/长时门禁仍保留，模拟通过不等于真实双应用验收通过。
