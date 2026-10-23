# Task8.2：课程分析共享额度

2026-10-23；基线087aae5。课程分析尚未开放UI/任务入口，本页不代表Task8整体完成。

## 变更

- TranslationService内部bool用途改为固定枚举，沿用同一SQLite预留/结算事务；增量course_usage分项，不迁移或清零旧usage、creative_usage。
- 课程请求受原DeepSeek全用途月度上限约束。课程、创作、翻译用量分别展示；总量减两类分项计算翻译用量，兼容旧返回中没有courseTokens。
- 课程每次只发送一次，不自动修复、重试或跳转HTTP重定向。新付费重试的预览许可留8.3—8.4实现。401等明确拒绝请求释放预留；5xx、响应读失败/超限、缺usage、发送后取消保守保留。完整有效usage先结算，再交由结果解析器处理。
- 资产分析仍最多4图；课程专用预算最多6图、单图350000字符，文字请求上限120000字节，max_tokens为1—16384。课程HTTP外层响应最多2MiB，8.1结果JSON仍最多1MiB；不全局扩大旧翻译或资产限制。
- 未创建额外队列/服务/数据库schema，未读取或更改源项目。

## 测试过程

忽略目录release-artifacts/yingliu-merge/保留日志：

|验证|结果|
|---|---|
|task8-budget-red.log|4行为测试在未实现stub处失败|
|task8-budget-green.log|沙盒回环连接失败，3失败；不视为业务验证|
|task8-budget-green-native.log|正常权限本地HTTP：4通过，实际连接计数证明503不二次发送|
|task8-usage-ui-red.log → task8-usage-ui-green.log|用量归类断言失败 → 3通过|
|task8-budget-full-rust.log|374通过、0失败、2既有忽略，71.62秒|
|task8-budget-full-ui.log|沙盒临时模块目录ENOENT，69文件未加载，不计为通过|
|task8-budget-full-ui-native.log|正常权限342通过、1失败：旧费用确认框竞态|
|task8-editor-reproduce.log|单独旧编辑器测试再次失败，受影响用例变化，确认时序问题|
|task8-cost-race-red.log → task8-cost-race-green.log|layout阶段提交稳定复现 → 编辑器及用量18通过|
|task8-budget-final-ui.log|70文件、344通过、0失败，41.99秒|
|task8-budget-final-build.log|生产构建退出0；保留已有大chunk提示|

费用确认竞态根因：指纹变更的被动effect无条件invalidate并清空cost，可能在同一新指纹已开启预览之后执行。改为仅清理指纹不匹配的旧cost，保留已有异步响应绑定核对；旧库/旧输入仍被丢弃，不放宽付费批准。

fmt及diff检查退出0；证据加入前秘密扫描396文件/0，提交前再检查。没有真实付费请求、发布或安装。仅回环模拟服务，不能称作真实供应商端到端验收。新course_json_with_usage在8.4接任务前暂有dead_code提示，未用注解隐藏；已有构建chunk/cache/linker提示保留。

## 下一步与边界

8.3资料快照、24小时确认预览和缓存身份；8.4阶段任务；8.5分析UI仍待实现。物理DPI/跨屏/长时门槛保持最终验收，不自动合并或发布。
