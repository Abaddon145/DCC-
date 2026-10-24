# Task 9.3a：后台 Markdown 纯转换边界

日期：2026-10-24。BASE `0239e21`，分支 `codex/yingliu-learning-integration`。此阶段完成纯转换器，没有启用转换提交入口、读取真实源数据或完成整项 Task 9。

## 实现边界

- `convert_source_note` 仅接收有界字节和后台明确提供的附件、课程映射；不读文件、数据库或网络，也不调用 AI。输出既有 `document_json` 节点、完整原字节 SHA-256、待处理状态、警告与需保留的原始 UTF-8 字节范围。
- 标准标题、段落、粗斜体、删除线、代码、引用、普通/有序/待办列表、无对齐属性的表格、HTTP/HTTPS 链接直接进入既有编辑模型。托管图片和文件必须有明确 UUID 映射，不能凭文件名猜测或下载远程附件。
- HTML、注释、frontmatter、Wiki、公式、脚注、特殊引用/标题/表格/代码属性、缺附件与未知课程保留完整最外层块。不会执行 HTML，也不留下半个转换块。跨段引用定义先由全篇解析解决链接目标，其非空源定义间隙仍保留为待处理原文。
- BOM 从解析视图中略过，但摘要与偏移仍对应原件；CRLF 不改写归档。代码围栏的解析范围不包含结尾换行时，补入实际原始 CRLF/LF 字节，不生成替代换行。
- 旧 `yingliu://video/<sourceId>?t=<ms>` 只接受字面明确身份和整数时间，禁止借 URL 路径归一化、编码参数、额外 query 或越界时间静默换目标。可信课程映射生成 `[t,t+1)` 时间点节点，沿用课程 UUID、媒体 UUID、完整摘要和范围校验。
- 输入超过 5 MiB 拒绝；无效 UTF-8 返回待处理且没有伪造正文。超过 50,000 行、100,000 事件、64 层或目标 JSON 4 MiB 时退回完整原文块，不截断。660,000 字节普通文本已实际转换并通过现有知识文档校验。
- 占位块 ID 只是草案；尚未登记的片段仍会被既有保存层拒绝，必须由后续 9.3b 登记并 remap。附件归属、当前课程身份和库/修订/来源快照的再次校验也属于后续提交边界，不能把纯函数测试当作真实入库验收。

## 依赖与共用语义

Rust 后台任务不能依赖 WebView 在线执行解析。锁定 pulldown-cmark 0.13.5，关闭默认特性，只消费带原始偏移的事件；实际特性树没有启用 CLI、HTML renderer 或 SIMD。新增传递依赖只有 unicase 2.10.0；既有 bitflags、memchr 复用锁定版本，Cargo.lock 仅新增两个包及根依赖。

上游固定提交、MIT 许可及最小 Rust 版本已核对，build.rs 在未启用 gen-tests 时为空操作。两份完整许可复制入应用资源，并在 Tauri 资源列表和第三方声明登记；复制后 SHA-256 与原许可一致：pulldown-cmark `C4F10F55904BDB9F27D3FBF94C354926D6CFE8B982276E556238C258941B243B`；unicase `4F6BD11A0F17FE5B085ACE1DAAEDB7B002A08316850DD9577C246C6A68A68E8C`。

保留现有手动 MarkdownManager 路径。四组共享 fixture 分别核对标题与标记、嵌套列表与引用、待办、普通表格；Rust 与前端使用同一输入和期望语义，不比较编辑器补充的默认属性。未引入第二个日常编辑器、正文存储或新数据库表，schema 仍为 v27。

## RED → GREEN 与验证

- 转换桩：7 组全部失败，`task9-conversion-red.log`。首轮实际转换 6 通过、1 失败：测试误将脚注引用段与独立定义看成一个外层块，已改为分别核对两段原字节，没有放宽内容保留要求。
- 新边界先 7 通过、2 失败：URL 解析器会归一化路径/编码参数，围栏额外属性也会被忽略。改为字面身份校验和完整原文保护。随后发现围栏结束偏移不含 CRLF，补入实际原字节后，9 组全部通过，0.32 秒。日志 `task9-conversion-edge-red.log`、`task9-conversion-edge-green.log`、`task9-conversion-final-targeted.log`。
- 前端共用 fixture 与既有 Markdown：10 项通过，1.41 秒，`task9-conversion-shared-ui.log`。
- 完整 Rust：`cargo test --offline --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`，**446 通过、0 失败、2 既有 ignored**，123.74 秒，main/doc tests 0，进程 93383 退出 0。日志 `task9-conversion-full-rust-permitted.log`。
- 完整前端：`npm test`，**380 项 / 73 文件通过**，45.95 秒；`npm run build` 退出 0，8.86 秒，保留既有大 chunk 提示，进程 15688 退出 0。日志 `task9-conversion-full-ui-permitted.log`、`task9-conversion-build.log`。
- 首次完整沙箱测试失败：前端 72 个套件遇到沙箱临时文件 ENOENT，Rust 的原子文件/文件锁等测试遇到权限拒绝。按权限规则在沙箱外重跑同样输入、未改业务逻辑，上述完整验证全部通过；失败日志保留，不作为通过证据。
- fmt/check、工作区 diff/check、依赖特性检查退出 0；最终凭据扫描 438 个文件、零命中，排除无关 `Luca/`、`outputs/`、`tmp/`。暂存后标准 diff/check 仅提示 unicase 官方许可的原有末尾空行；保留其逐字节许可，使用仅关闭 blank-at-eof 提示的检查确认其他空白问题为零，没有改写上游许可来消除该提示。

本阶段没有推送、合并、发布、覆盖安装或下载模型；没有修改映流代码、两份原方案、用户 Vault、数据库、凭据或笔记。Luna medium 只读核对既有节点规范，主线程顺序实现，没有使用 GPT-6 Astra。真实迁入、原生 GUI、IME/DPI/跨屏及最终分发验收仍按主计划单独报告。
