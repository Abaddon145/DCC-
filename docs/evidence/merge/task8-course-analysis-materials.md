# 课程分析材料：字幕优先、可选采样帧、可选本地 Whisper

2026-11-02。基线 `codex/yingliu-learning-integration` @ `9326008`。本轮补齐 Task8 已有课程分析链路上的材料选择，不新开第二套 AI 队列，不写 `asset_ai_analysis`，不读取映流 Vault，不下载模型，不发起付费请求。

## 行为

- 已导入且有段落的字幕优先。未指定字幕时，后端选择最新一条 `cue_count > 0` 的轨道；空轨道不能挡住仅采样帧的分析。
- 分段请求体只包含该段字幕正文。没有采样帧时不含 `image_url`，也不包含视频路径或 `data:video`。采样帧仍走既有单张 JPEG 提取，不是整段视频上传。
- 课程分析面板写明流程：准备材料 → 确认预览 → 分段分析 → 按需汇总。失败后必须重新预览。确认勾选之前不提交 `courseAnalysis`。
- 没有可用字幕时才检查本机 Whisper。工具和模型都已核验才允许勾选本机 CPU/磁盘并启动既有 `courseTranscription` 任务。未就绪时说明不会自动下载，安装入口仍在「课程字幕 → 本地转写」。转写完成只刷新字幕列表，不自动调用 `prepareCourseAnalysis`。
- 采样时间可留空。填写的每个时间只作为一张 JPEG 的时间点。

## 命令与结果

工具链：`rustc 1.89.0` / `cargo 1.89.0`（`src-tauri/Cargo.toml` 的 `rust-version`）。前端 `tsc -b` 退出码 0。

- `npm test -- src/features/learning/CourseAnalysisPanel.test.tsx src/features/learning/LearningWorkspace.test.tsx`：2 个文件、39 项通过。覆盖字幕加采样帧的确认前不付费、无字幕时未安装 Whisper 仍可预览采样帧、已核验 Whisper 只启动转写任务、转写完成后不自动预览、有段落的字幕优先于空轨道。
- `rustup run 1.89.0 cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_analysis_prefers_usable_subtitles -- --test-threads=1`：1 通过。默认材料使用已登记字幕，请求体含字幕正文，不含视频文件或图像 URL，课程用量仍为 0。
- `rustup run 1.89.0 cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_analysis_subtitle_preference -- --test-threads=1`：1 通过。较新的空轨道被跳过，较新的有段落轨道被选中。
- `rustup run 1.89.0 cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_analysis -- --test-threads=1`：34 项中 25 通过、9 失败。失败点是本环境既有的 Windows 限制，不是本轮材料选择：字幕导入拒绝以 `/` 开头的路径（「请选择本地普通字幕文件，拒绝网络、设备及备用数据流」）、单帧提取报「下载进程暂仅支持 Windows」，以及随后的锁中毒和本地 mock HTTP 管道断开。新测试在该轮里通过。没有外部付费请求。

## 未完成

- DCC 资源里只有 `src-tauri/resources/whisper-components.json`。Whisper 1.9.1 可执行文件和 base/small 模型没有打进安装包，仍由用户在「课程字幕 → 本地转写」导入已核验文件。本轮没有执行真实语音识别。
- 没有真实 DeepSeek 调用，也没有在本 Linux 环境重新跑通 FFmpeg 抽帧。抽帧仍是既有 `sample_jpeg`，受 Windows 进程门限制。
- 没有改 PR #22，没有合并到 `main`。
