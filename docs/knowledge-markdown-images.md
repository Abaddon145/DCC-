# Markdown 图片导入

Markdown 导入中的本地图片和公开 HTTP/HTTPS 图片会复制到当前素材库的笔记附件，正文使用内部附件地址，下载完成后可离线查看。

- 网络图片逐张下载，不携带 Cookie、账号或授权信息。
- DNS 与每次跳转均检查公网地址并固定解析结果；拒绝内网、本机、凭据 URL、非常规端口及 HTTPS 降级。
- 每张最多 25 MB / 2500 万像素，最多 100 张，下载总量限制 300 MB，单张和整体有超时。
- 图片通过实际格式和解码验证，仅接受 PNG、JPEG、WebP、GIF。
- 下载失败保留正文和图片占位，可在编辑状态点击“重试下载图片”；成功后正常自动保存。
- 图片随素材库备份与迁移保存；原始 Markdown 不被改写。

2026-09-30 使用用户提供的 Markdown 在独立临时素材库验证：10/10 网络图片下载、导入成功，清理下载缓存后仍可读取。用户文件和图片不提交到仓库。

真实网络验收测试默认忽略；设置 `DCC_NOTE_ACCEPTANCE_MD` 后运行 `cargo test --lib real_markdown_images_are_managed_offline -- --ignored --nocapture`。普通回归不访问真实网站。Windows 界面验收仍由测试安装包进行，不作为正式 Release 发布。
