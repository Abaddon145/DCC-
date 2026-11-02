# Task9.3b3：受控预览内容

基线 d943661；schema28、依赖不变。只在已有预览与媒体协议上补齐读取能力，不激活迁入页面。

- 图片地址固定为 `note-preview/<library>/<preview>/<placeholder>`。核对当前库/位置/维护状态、快照完整性、图片归属与 MIME、原件副本大小和 SHA；复用只读句柄和分块停止检查。额外路径段、伪造占位、其他库、非图片、篡改和取消/已提交预览均拒绝，响应 no-store/nosniff。
- 原文命令只接受库/预览/原文块 UUID，重新验证归档全摘要和精确字节范围；返回 UTF-8 纯文本，不执行 HTML、不登记新片段、不写正文。取消预览后原文仍保留可核对。
- 图片 IO 在后台连接上执行，不持有全局素材库锁；返回前再次检查库作用域。正式笔记附件原协议不变，预览占位不会冒充正式附件。

## 验证

- 三项读取测试对桩 RED；首次沙箱因临时库 persist 权限失败，不计产品 RED。在允许环境重跑后，明确因尚未实现的读取能力失败，`task9-preview-content-red.log`。
- 后端四项定向 GREEN，0.45 秒，进程15458退出0；`task9-preview-content-green.log`。后续只增强提交后禁止图片读取、取消后仍可核对原文的断言，随完整回归通过。
- 前端新增原文路由先 RED，再连同已有四条 API 共五项 GREEN，0.795秒，退出0；`task9-preview-content-api-red.log` / `task9-preview-content-api-green.log`。
- 完整 Rust **467通过、0失败、2既有忽略**，130.47秒，main/doc0，进程53306退出0；`task9-preview-content-full-rust.log`。
- 完整前端 **385项 / 74文件通过**，48.76秒；生产构建退出0，8.23秒，进程13084退出0；`task9-preview-content-full-ui.log` / `task9-preview-content-build.log`。既有大chunk提示保留。
- fmt/diff检查通过；凭据扫描排除Luca/outputs/tmp，446文件0命中，新增本证据后提交前再次核对。日志在忽略的release-artifacts/yingliu-merge。

测试使用真实临时库及AppState，未运行真实桌面图片加载或迁入UI；没有访问源用户库/Vault、付费AI、推送、合并、发布或安装。历史原件浏览/导出/计数、实际只读转换页面和Task14物理验收仍未完成。只读事实核查使用GPT-6 Luna medium，未使用GPT-6 Astra。按用户karpathy-guidelines限制修改范围，未创建通用文件读取服务。
