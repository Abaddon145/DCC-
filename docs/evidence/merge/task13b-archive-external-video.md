# Task 13b：外部视频原件归档复制

2026-10-29。基线 `2fc2061`。本记录只覆盖 Task 13 / M6B 里尚未落地的 `archive_external_video`。批次撤回仍以 `task13a-import-revert.md` 与 `task14b-revert-recovery.md` 为准。测试只使用临时目录里的合成字节，不读取真实映流 Vault 或用户库。

## 行为

- `archive_external_video(expected_library_id, entry_id, operation_id) -> TaskId` 把「归档外部视频」放进现有 `TaskRegistry`。同一素材库、同一条目上已有进行中的任务时，相同 `operation_id` 返回原任务；另一个操作被拒绝。通用任务入口不能绕过这个命令。
- 复制目标是 `media-library/video/<entry>/files/<原文件名>`，使用现有 `copy_atomic`。磁盘余量用现有 `check_available_space`：Windows 读卷空闲空间，Linux 测试读 `statvfs`。空间不足时登记保持外部，恢复日志为 `pending`，不写托管文件。
- 托管文件摘要与登记的 SHA-256、大小一致后，同一事务删除 `media_external_files` 行并把原 `media_files.rel_path` 从 `external-references/<file>` 改为托管路径。不插入新的媒体条目、媒体文件或资产核心。随后删除该文件的本机 grant。
- 源路径上的原件不删除、不改写。源内容在复制后不再匹配登记摘要时返回 `SOURCE_CHANGED`，外部登记保留；若托管副本已经对上原摘要，日志记为 `ready`，把源字节恢复后可用同一 `operation_id` 完成切换。
- 复制完成后数据库提交失败时，外部登记和 grant 都还在，托管副本留下。同一操作重试只提交事务，不覆盖已校验的副本，也不产生第二份登记。
- 取消发生在复制完成之前时，外部登记保留，不留下托管文件。已经切换成功之后才收到停止请求的任务按完成记录，并说明源文件仍保留。
- 恢复日志在 `.dcc/runtime/imports/archive-external/`，不进整库包。已经复制进 `media-library/` 的文件会随备份走。归档后外部原件计数为 0；恢复出的库不靠 grant 也能按托管路径读到相同字节。

## 测试（先 RED 再 GREEN）

`media_archive::learning_import_recovery` 共 6 条：

- `archive_copies_managed_bytes_and_keeps_identity_without_deleting_source`
- `archive_low_space_keeps_external_reference_pending`
- `archive_rejects_replaced_source_and_keeps_one_registration`
- `archive_cancel_keeps_source_and_registration`
- `archive_commit_failure_retries_without_a_second_registration`
- `archive_task_reuses_operation_and_backup_restores_managed_file`

RED：实现入口返回「外部视频归档尚未实现」，6 条全部失败（`release-artifacts/yingliu-merge/task13b-red.log`）。GREEN：同一过滤器 6 条通过（`task13b-green.log`）。

界面：视频详情在主文件 `storageMode=external` 时提供「归档复制到本库」，确认后按当前库排队，并说明源文件仍保留。托管视频不显示该按钮。设置里的未归档提示补充了这条入口。`ArchiveExternalVideo.test.tsx`、`StorageSettings.test.tsx`、`MediaDetailCapture.test.tsx` 共 7 条通过（`task13b-frontend.log`）。

## 验证

- 归档专项 6 条通过，且包含在 Rust `--lib` 全量结果里。
- Rust `--lib --test-threads=1`：488 通过，134 失败，2 忽略（`task13b-rust-full.log`）。失败集中在本机没有 Windows 进程、`LOCALAPPDATA`、DPAPI 或捆绑 `ffmpeg.exe` 的既有用例，例如「下载进程暂仅支持 Windows」。归档 6 条均为通过。`managed_resolution_is_unchanged` 仍通过。
- 前端相关 3 个文件、7 条通过（`task13b-frontend.log`）。前端全量 83 个文件、482 条通过（`task13b-frontend-full.log`）。`cargo fmt --all -- --check` 与 `git diff --check` 通过。
- 为了让 Linux 能编译库测试，给 `runtime/process.rs` 里依赖 PowerShell / `windows_sys` 的既有测试加了 `#[cfg(windows)]`。这些测试在 Windows 上仍会编译并运行。

## 验收边界

- 生产迁入入口仍是单篇 Markdown。本轮没有把外部视频归档接到迁入批次，也没有批量归档整个库。
- 打开中的视频详情不会在任务完成后自动重读；完成后以任务中心结果和重新打开详情为准。
- 最终摘要确认发生在持有素材库锁期间。大文件会在这段时间挡住同库写入。复制本身不持有该锁。
- 恢复日志不在整库备份里。备份发生在「文件已复制、数据库尚未切换」时，恢复后的库仍显示外部登记，但 `media-library` 里可能已有副本；同一操作或新的归档可以按摘要完成切换。
- 真实桌面、DPI、真 Vault、付费 AI、NSIS、安装和合并到 main 都没有做。合成字节不能代替这些验收。
