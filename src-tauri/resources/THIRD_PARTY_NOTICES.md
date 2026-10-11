# 第三方组件与许可证

栈藏 DCC 素材库 v0.21.0 随安装包或前端产物使用以下第三方组件：

## 下载工具（Phase 1）

- yt-dlp Windows x64 2026.08.19：独立进程调用；上游源码采用 Unlicense，但本次 PyInstaller Windows 二进制包含 GPLv3+ 组件，完整组合须按 GPLv3+ 分发，不能将此 exe 标成 MIT/Unlicense。随包附带 `media-tools/YTDLP-UNLICENSE.txt`、`YTDLP-GPL-3.0.txt` 和 `YTDLP-THIRD-PARTY-LICENSES.txt`。
- 源码/构建/版本资料：https://github.com/yt-dlp/yt-dlp/tree/2026.08.19 、https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19 。分发者须遵守组合组件的对应源码与版权通知义务；未将下载工具链接进 DCC。
- Deno 2.9.7 Windows x64：MIT，独立 JavaScript 运行时，供 yt-dlp 解析源站脚本；完整许可证见 `media-tools/DENO-LICENSE.md`，源码与依赖资料：https://github.com/denoland/deno/tree/v2.9.7 。
- 固定版本、SHA-256 和来源见 `media-tools/DOWNLOAD-TOOLS.md`。没有在线自动更新或任意插件执行入口。

## FFmpeg / FFprobe 9.0.2

- 构建来源：BtbN/FFmpeg-Builds `ffmpeg-n9.0.2-win64-lgpl-shared-9.0`
- 上游：https://ffmpeg.org/
- 构建项目：https://github.com/BtbN/FFmpeg-Builds
- 许可证：GNU Lesser General Public License v2.1 或更高版本（LGPL 构建）
- 用途：预览视频代理、首帧缩略图、音频代理与波形图。

FFmpeg 作为独立可执行程序由 Rust 后端以固定参数调用，不与应用代码静态链接。完整许可证文件随 `media-tools` 目录分发。

## Three.js

- 上游：https://github.com/mrdoob/three.js
- 许可证：MIT License
- 用途：本地 GLB、GLTF、FBX、OBJ 与 STL 预览。

完整 Three.js MIT 许可证随安装包以 `THREE-LICENSE.txt` 分发，版权归 Three.js 作者及贡献者所有。

## Tiptap 3.31.3

- 上游：https://github.com/ueberdosis/tiptap
- 许可证：MIT License，Copyright (c) 2025, Tiptap GmbH。
- 用途：本地富文本笔记编辑、表格、待办、图片和 Markdown 解析/序列化。
- 未使用 Tiptap 的付费协作、AI、云端历史服务。
- 完整许可证随安装包以 `TIPTAP-LICENSE.txt` 分发。

## 产品参考

## Mind Elixir 5.15.1

- 上游：https://github.com/SSShooter/mind-elixir-core
- 许可证：MIT，Copyright (c) 2019 DjZhou。
- 用途：笔记中独立可编辑思维导图，按需加载，不连接在线服务。
- 完整许可证随安装包以 `MIND-ELIXIR-LICENSE.txt` 分发。

Markdown 解析使用 Marked 17.0.6（MIT，MarkedJS / Christopher Jeffrey），许可证见随包 `MARKED-LICENSE.md`。

知识库设计参考 Trilium、Joplin 和 Logseq 的产品交互，独立实现持久化、历史、附件和 AI；未复制其 GPL/AGPL 代码。
