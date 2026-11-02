# 栈藏对映流联动协议 v1 的适配审阅

日期：2026-10-21。状态：**已审阅提议版 1.0.0，存在待统一项，尚未确认可进入 Phase 5—6。**

栈藏基线 `48078e56e0b304dc313a03c9aba4ef010847dacb`（v0.23.0、schema 22）。本文件仅提出审阅意见，不修改映流协议，不作为替代协议。

## 审阅的原件

- 文档：映流 `docs/integration/yingliu-dcc-v1.md`，SHA256 `2136D9D1F0C2988502D066F6A1464392C74927878731BE879412FA8EA8B42F33`。
- 共用样例：映流 `docs/integration/yingliu-dcc-v1/golden.json`，SHA256 `91C96869BD50B64BFECD2D58CF1889DEDD4B6FDAD6BB24F08E764A2366F221FD`。
- 上述文件在本窗口只读。双方确认记录仍为空；本端未声称联动已实现。
- 静态检查：golden.json 可解析，含 5 个请求、25 个场景和 3 个回执样例；这些计数及人工审阅不等于运行时协议一致性测试。

## 可直接适配的部分

完整库/实体身份、毫秒时间、版本白名单、受限交换目录、启动仅 UUID、显式授权、附件安全、持久回执及请求内容去重符合本端要求。栈藏可复用已有 UUID、项目/媒体关联、笔记附件、参考板快照和任务维护屏障。无需引入另一套媒体库、项目系统或在线 AI。

## 进入实现前需统一

| 编号 | 证据/影响 | 请协议拥有方补充或确认 |
| --- | --- | --- |
| B1 返回媒体动作缺失 | 第 2 节 `openMedia` 只允许 dcc→yingliu，现有其他动作只有 openNote/openProject，映流无法准确返回栈藏媒体详情。用户明确要求准确打开项目/素材。 | 为返回栈藏媒体定义方向及 payload；建议在既有动作上明确只传拥有方媒体身份的返回语义。返回请求不应伪造原件 path 或重新触发映流打开。补共用样例。 |
| B2 有效样例与本端 UUID 冲突 | 第 1 节 entityId 泛化为可打印字符串；golden 有效场景使用 `media-test-1`、`project-test-1`、`dcc-note-test-1`。本端 ID 是 UUID，核心命令先校验 UUID、笔记引用也验证 UUID。按实际后端验证这些“有效”样例会拒绝。 | 允许各拥有方做自身 ID 约束，栈藏有效样例改为 UUID；映流稳定 ID 可继续使用其原有格式。不要放宽本端 ID 验证迁就 fixture。 |
| B3 刷新状态无完整回传 | `linkKnowledge refresh` payload 有标题/updatedAt，但 receipt.result 仅 entities/disposition/warnings；协议未定义谁验证外部笔记、怎样带回 available/stale/unavailable、最新标题和核对时间。其 disposition 也没有 refresh 对应值。 | 明确 refresh 方向、拥有方返回字段和状态含义，未运行/未核对不能标记 available。增加笔记移动/删除/恢复、离线、晚到响应共用样例。只同步显示快照，不含全文。 |
| B4 摘录重复与目标变更 | 去重键不含 destination；同 excerptId/revision 发到另一笔记或参考板会命中原目标。若仍报 completed，用户可能以为写到了新目标。 | 明确同修订换目标是 CONFLICT，还是允许多目标分别接收；原回执必须展示原目标，不能成功伪装写入新目标。补不同 requestId、相同摘录但不同 destination 的样例。 |
| B5 媒体 ID 覆盖范围 | 本端有 `asset_core.id`、`assets.id`、`asset_media.id`、`media_entries.id`、`media_files.id`；都是不同用途。协议只写 `dcc/media`，尚未写清是媒体条目还是所有资产。 | 首发建议固定 `dcc/media.entityId = media_entries.id`，path 为其登记的主原件，排除代理和缩略图。从核心资产入口先解析 origin。普通素材的旧预览附件若也在首发范围，应另列身份语义，不能把 attachment UUID 当媒体条目 UUID。 |
| B6 去重寿命与恢复语义 | 本端整库备份恢复到另一库时保留目标 library.json UUID，但恢复实体 UUID；手工复制可能保留 UUID。协议要求“复制成独立库生成新 ID”，当前应用并非所有外部复制都能自动识别。回执在新库上重放的归属尚未约定。 | 明确以当前实际库 marker 为准；无法确认库归属时拒绝并要求显式重新定位。区分同库原地恢复与恢复到新库；从备份恢复的配对/发送记录不能自动授权机器操作。补两类恢复测试，不能只按 entityId 找目标。 |

这些问题属于当前草案与真实本端的接口缺口；目前尚未实施传输，不宣称它们是已发生的运行时故障。

## 实现前还需细化的安全/恢复细节

- 固定命名管道名称/SID 派生规则、配对身份的存储与验证、请求目录发布者和 receipt 写入方、ACL 所有者/DACL 判定及目录迁移方式。现有采集邮箱只有 3 秒轮询，不能直接当作 p95≤1s 的实体导航传输。
- 指定 canonical request hash 的算法（字段顺序/空白/数字/Unicode/未知字段），否则两端可能把等价 JSON 当作 REQUEST_ID_CONFLICT。接收记录未确认前不得只比较原始 JSON 字节。
- 明确旧笔记追加摘录的确认与修订保护，摘录更新只改本次接收拥有的块。接收方已有编辑时返回 CONFLICT；不能让 linkKnowledge refresh 修改正文。
- 恢复包需固定格式、清单/附件大小上限和路径规则；导入仍走外来确认，不能因有 requestId 自动写入。
- 共用样例缺少真实合法图片/成功 board、修订冲突、零时长/超界、失败恢复及源文件变化等的精确数据，目前部分 case 只有文字场景。建议用共享生成说明补齐，静态 JSON 不充当文件系统测试。
- 明确校验顺序：错误库、过期、非法目标、board 无图等多错误输入的预期码，以及 completed 请求的终态查询优先于过期检查。

## 本端已确认事实

1. v0.23 的 `asset_core` 是兼容索引；交接须解析 origin，不能重建或覆盖它。
2. 主应用通过 `browser_capture::application_lock` 持有 OS 文件锁；第二实例当前只写 `open.json` 并退出。`main.rs` 尚无 `--handoff-request` 分支。
3. 前端已有精确实体打开入口，但外部导航必须先核对当前库、模块、维护状态和笔记未保存草稿。
4. 笔记现有本地引用只接受 note/asset/media/project，尚无映流课程、外部笔记、时间码卡。新增表示需与双方最终字段对齐。
5. 本次新增的双临时库备份恢复验收检查实体映射、库 marker、媒体 hash、笔记/项目引用和参考板；没有操作用户真实库。

## 确认门槛

映流窗口修订其协议及 golden 样例，或给出对以上条目的明确裁定后，本端重新读取、重新记录 SHA256，再向用户报告一致性。双方确认相同版本后才实施本端 Phase 5—6。当前没有自动合并、发布或覆盖用户安装。
