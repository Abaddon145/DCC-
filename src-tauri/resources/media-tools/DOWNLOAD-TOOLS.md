# 下载组件固定版本清单

| 文件 | 版本 | SHA-256 |
| --- | --- | --- |
| yt-dlp.exe | 2026.08.19 | 66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a |
| deno.exe | 2.9.7 | e020f3e232bd16e33768dee528e5983349c962952051ced0a5d58ad42f5d9b33 |

下载核心来自映流的能力设计，DCC 实现按自己的任务系统重新编写。以上两个已固定版本的工具从本机映流资源目录复制，原仓库未改动。执行前校验哈希；校验失败拒绝运行。

- yt-dlp 发布与源码：https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19
- Deno 发布与源码：https://github.com/denoland/deno/releases/tag/v2.9.7
- yt-dlp Windows 打包文件为 GPLv3+ 组合（详见其 README Licensing 与随包第三方许可证），不是单纯的 Unlicense。
- 使用 DCC 现有 FFmpeg / FFprobe 9.0.2 LGPL shared 构建，没有迁入映流的另一套 FFmpeg。
- GPL、Unlicense、第三方许可证及 Deno MIT 许可证位于此目录，并随中文 NSIS 分发。
- 不读取项目内容，不自动升级工具，不加载 yt-dlp 用户配置或插件，不支持任意命令选项。
