# Task8.6a：可选本地转写的离线边界

2026-10-24；基线 d8b1c21。延续已批准的主 Task8 及课程分析子计划，不引入新队列、数据库或付费接口。本检查点提供后端可运行的校验/解析接口，尚未接入转写任务、组件安装或产品操作入口，不能称为语音识别功能已交付。

## 实现

- 固定清单编译进程序：whisper.cpp 1.9.1、Base/Small 模型 revision `5359861c739e955e79d9a303bcbc70fb988958b1`。只接受清单内模型 ID，不读取调用方自报的可信摘要或下载地址。
- exe、全部已登记 DLL、所选模型均做完整 SHA256；模型另验证精确长度与 GGML 头。拒绝路径穿越、ADS、网络/设备路径、junction/重解析点及未登记 exe/DLL；额外 CPU DLL 不可因自动发现机制混入。Windows 只读锁随校验结果保留，取消不返回成功。
- Whisper JSON 最多32MiB，完整段最多200000，单段正文最多16000 UTF-8字节。只接受真实已知时长内、有序的非负整数毫秒。遇到坏行/空正文整体失败，不跳过坏行、不夹断时间。
- 转写段复用 `TranscriptCue` 的稳定字幕 ID/段序号。完整输出 SRT 最多8MiB，交给既有字幕解析器做无损 round-trip；无法表示的空行/控制字符明确失败。没有复制第二套字幕持久化逻辑。
- 来源仅从映流固定提交 c86650831aecb55ef7bec288df9ae7fb27c8d178 的 transcription.rs、whisper_models.rs、ai-components.json 只读核对；现有 YINGLIU-LICENSE.txt 与第三方说明继续适用。没有分发 Whisper 二进制或模型，未来实际分发必须附组件自身许可。

## 行为验证

日志均在忽略目录 `release-artifacts/yingliu-merge/`：

- `task8-transcription-boundary-red.log`：6项中4项因未接入的解析/校验 stub 失败；实现后6项通过。
- `task8-transcription-dll-red.log`：额外 `ggml-cpu-extra.DLL` 原先仍能通过校验；增加已登记组件目录检查后通过。
- `task8-transcription-size-red.log`：恰好8388608字节的有效 SRT 被保守预估误拒；改为实际编码大小校验后通过。
- `task8-transcription-nine-targeted.log`：9项通过，包含中文/emoji/多行、真实毫秒、模型同大小篡改、缺 DLL、未知 ID、恶意文件名、错误 GGML 头、junction、持锁拒绝写入，以及8MiB精确边界。微型合成文件仅在测试中使用，未加入产品允许清单，不是可用模型或可执行工具。
- 首次 junction 用例被沙箱拒绝创建链接；普通权限环境重新执行通过。未放宽产品检查、未跳过该用例。
- 取消断言自查后移到工具/模型仍然完好时，并检查取消错误，避免从“文件已篡改”误得取消测试通过；提交前完整回归覆盖该收紧。

## 回归及限制

- 首轮完整 Rust：409通过、0失败、2既有忽略，119.78秒，main/doc各0；`task8-transcription-full-rust.log`。收紧取消测试后的提交前完整回归 `task8-transcription-commit-rust.log` 同为409通过/0失败/2既有忽略，118.60秒，进程退出0。既有忽略为20k笔记基准和真实 Markdown 图片网络验收。
- 首次前端回归70个文件被沙箱临时缓存 ENOENT 阻断（1文件15项通过）；普通权限重跑 `task8-transcription-full-ui-permitted.log`：71文件357项通过，45.90秒。该环境错误未当作产品失败或成功忽略。
- `task8-transcription-build.log`：生产构建退出0，4.16秒；保留既有大 chunk 警告。本检查点接口尚未接产品入口，Rust 的 unused/dead_code 与 linker 输出保留在日志中，没有用 allow 属性隐藏。
- 本轮只有一名只读事实子体 GPT-6 Luna medium 核对工具打包/备份/许可；普通实现与测试由主会话顺序进行，没有 GPT-6 Astra 或额外审查群。
- 没有执行 Whisper、下载安装模型、读取真实用户库、改动映流/Vault/用户原件、调用付费服务、推送、合并、发布或覆盖安装。Task8.6b/c 的同一任务系统、显式安装/导入入口与真实模型识别仍未完成；物理DPI/跨屏等仍在 Task14 集中验收。
