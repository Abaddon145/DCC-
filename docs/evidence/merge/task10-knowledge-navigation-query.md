# Task10c-K1：知识导航查询

基线6df1966。补真实过滤能力，不改schema、既有正文或项目关联表；前端导航按钮尚未接入。

- 原search/search_ordered签名保留，委托共享search_navigated的空导航过滤。既有FTS/LIKE、收藏、回收站、笔记本、手动排序和分页继续同一实现。
- 收件箱通过notebook_id IS NULL筛整个结果，再分页；项目视图匹配正文直接登记的project引用，不推断素材关联，EXISTS避免多次引用重复计数。count和列表使用同一WHERE。
- 新视图仅最近更新排序；手动排序请求明确拒绝。项目ID需规范UUID且在当前库存在，非法/未知ID不回退为全量。search_notes增加可选unfiledOnly/projectId，旧客户端缺字段默认保持原逻辑。

验证：两项实际SQLite检查对桩RED，随后GREEN，0.05秒，进程90411退出0；覆盖关键词、收藏、回收站、页间去重与总数、旧All/笔记本排序、真实项目直接/重复/错误类型引用、组合过滤及非法范围。首次编译误用了不存在的Result方法名，编译器E0599拒绝后改为既有is_ok_and模式，未放宽UUID判断。

- 完整Rust **473通过、0失败、2既有忽略**，126.60秒，main/doc0，进程91279退出0；`task10-knowledge-filter-full-rust.log`。
- 完整前端 **410项/77文件通过**，49.05秒；生产构建退出0、4.07秒，进程82779退出0；`task10-knowledge-filter-full-ui.log` / `task10-knowledge-filter-build.log`。既有大chunk提示保留。
- fmt/diff通过；凭据检查排除Luca/outputs/tmp，新增证据前461文件0命中，提交前复核。日志在忽略的release-artifacts/yingliu-merge。

复杂知识库布局接缝由GPT-6.1 Sol high只读核查；主线程实现，未使用GPT-6 Astra。后续仍需实际导航和右侧信息区接入、编辑器草稿/选区/undo/历史切出验证，以及Task14真实显示和输入法验收。没有源用户数据、网络付费调用、推送、合并、发布或安装。
