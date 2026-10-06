# 第三方组件与许可证

栈藏 DCC 素材库 v0.21.0 测试版随安装包或前端产物使用以下第三方组件：

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

UE 资产库的产品交互参考 UE5Box，独立实现；不包含 UE5Box 的代码、素材或品牌资源。

## Kura 原生缩略图桥接

- 上游：https://github.com/Yashashin/Kura
- 固定提交：`f8f6d06bf2a626f94cb10b0104b71ce80c6c5458`
- 许可证：MIT；完整许可证位于 `ue-tools/StackAssetBridge/KURA-LICENSE.txt`。
- 复用最小原生缩略图组件，并加入匹配版本、真实对象路径、完整 Editor 渲染与失败校验；没有复制 Kura UI。
- 不捆绑 Unreal Engine。辅助工具由用户已安装并有权使用的 UE / C++ 工具链本地编译，遵守 Epic 的相应许可。

## Mind Elixir 5.15.1

- 上游：https://github.com/SSShooter/mind-elixir-core
- 许可证：MIT，Copyright (c) 2019 DjZhou。
- 用途：笔记中独立可编辑思维导图，按需加载，不连接在线服务。
- 完整许可证随安装包以 `MIND-ELIXIR-LICENSE.txt` 分发。

Markdown 解析使用 Marked 17.0.6（MIT，MarkedJS / Christopher Jeffrey），许可证见随包 `MARKED-LICENSE.md`。

知识库设计参考 Trilium、Joplin 和 Logseq 的产品交互，独立实现持久化、历史、附件和 AI；未复制其 GPL/AGPL 代码。
