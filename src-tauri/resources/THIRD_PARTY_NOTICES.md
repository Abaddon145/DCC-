# 第三方组件与许可证

栈藏 DCC 素材库 v0.13.0 随安装包或前端产物使用以下第三方组件：

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
