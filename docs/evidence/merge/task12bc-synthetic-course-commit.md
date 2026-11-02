# 合成课程预检批准与提交

2026-11-02。接在 `0e38dcd`（合成只读门）之后。只对带 `dcc-synthetic-course-source.ok` / `dcc-synthetic-course-source-v1\n` 的 schema 8 样本增加一次性预检批准和 `commitCourse` 执行。不打开真实映流 Vault，不新增接受用户路径的 Tauri 命令。笔记迁入和外部视频归档的写入路径未改。

## 现在允许

`approve_synthetic_course_preflight` 与选择、撤销、提交共用 `synthetic_course_source` 这把进程内短锁。它先复制带标记的来源，只在副本上预检，再把批准留在内存里。批准约 15 分钟、一次性、不落库。它绑定当前 scope、profile、epoch、selection，以及来源字节摘要和课程源/目标指纹。预检不写课程、批次或媒体文件。媒体只能显式绑定当前库已有视频的 UUID；路径字符串和未登记 UUID 在授权前被拒绝。

`queue_synthetic_course_commit` 只接受选择 UUID 和预检 UUID，并排入现有 `commitCourse` 任务。重复排队返回同一条仍在执行的任务。直接 `tasks.submit` 出来的同形任务不会获得这条批准。通用 IPC 和通用重试仍然拒绝 `commitCourse`。

非 Windows 的 worker 执行该任务时重新复制并核对来源与目标，再调用既有 `stage_course_batch` / `commit_course_batch`。过期、取消、进程内批准被清空（对应重启）、切库、重新选择、来源或目标指纹变化，都要重新预检。成功后的撤回仍走既有 `revert_course_batch`。非 Windows 的课程 Tauri 命令仍返回仅支持 Windows，前端不能传入路径。

预检 JSON 沿用现有课程界面已经消费的形状：系列与课程、显式媒体 UUID、来源进度和目标进度、冲突码。绑定变更后的下一次预检会清掉旧批准。任务中心回执和批次刷新仍由现有 `commitCourse` 结果驱动，本切片没有改这些组件。

## 仍然不做

- 没有 Windows 拒绝写入 SHM 租约。活动写入端下的 SHM 保护仍只在 `cfg(test)` 的 `import_snapshot` 里。本切片能拒绝的是两次读取之间字节变化（包括提交复制中被改写的来源），不是对一个稳定占用文件的写句柄抢锁。
- 标记或侧文件在两次检查之间出现又消失，仍然检测不到。
- 没有前端任意路径命令，没有读取用户 Vault，没有安装、发布或合并 main。
- 非 Windows 不能从界面走到这条合成提交；选择仍是库内函数。

## 验证

Linux，`rustc 1.89.0`，`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib`：

| 过滤 | 结果 |
| --- | --- |
| `synthetic_course_` | 11 通过、0 失败 |
| `synthetic_course_`、`learning::course`、`learning_import_runner`、`learning_import_generic` | 57 通过、0 失败 |

`cargo check --locked --lib` 通过，保留既有未使用代码警告。没有重跑前端 Vitest：课程预检、任务中心回执和批次刷新组件未改。没有在 Windows 上重跑依赖文件租约的产品迁入测试。
