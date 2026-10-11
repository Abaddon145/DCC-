# DCC v0.21 Phase 1 — Downloader Integration

> Execution: inline, using executing-plans and test-driven-development. The user explicitly requested direct implementation; no additional design approval gate. Baseline: c683f988081919b36c99b4c4f30cb7c7aed58b62. Branch: codex/v0.21-downloader-integration. Source repository is read-only.

## Scope

Add 下载中心: URL validation → metadata/formats → queued download → cancellation/pause/resume/retry → checked file → existing media import → provenance. No editor, compression UI, AI, new media library, deleted features, or source repository modifications.

## Interface decisions

- Extend `tasks.rs` / `TaskRequest::Download`, the existing persisted BackgroundTask equivalent. Retain its filename and all existing request variants. No second task queue or DownloadTask domain entity.
- Download stages live in `phase`; shared task status remains compatible. Paused/cancelled download states are terminal to the dispatcher but resumable explicitly. Pause stops the child tree and preserves `.part` files; resume invokes yt-dlp `--continue`.
- A download owns a UUID workspace in the current library `.dcc/runtime/downloads/`. Retry retains this workspace and checks completed output before contacting the source again.
- All executable paths resolve through bundled tools, not frontend arguments. Inputs are URL/validated format IDs/settings only. No shell invocation, arbitrary options, or executable hooks.
- Existing LGPL FFmpeg/FFprobe and media import are reused. Download video/audio/subtitle sidecars; subtitles are not a new media kind and no subtitle editor is added. Playlist children enter the same task registry.
- Cookie material is protected per Windows user, never included in task payloads, diagnostic messages or the library backup. Temporary Netscape cookie files exist only during child execution and are removed by RAII.
- Add only download_tasks/media_sources and necessary indexes. Existing media_entries owns downloaded media. Provenance is additive; identical media may have multiple sources.
- Downloads/settings integrate current sidebar and settings conventions. No global UI redesign.
- Existing attached worktree cannot be read within this sandbox; user requested modifying this checkout. Use a new branch in the clean current checkout, without altering the earlier worktree.

## Tasks

1. Download domain, request/metadata models, format/progress parsing, safe yt-dlp arguments and classified errors. Tests first for URL/format/playlist/secret handling.
2. Runtime tools/process control, cookie storage and strict ffprobe analysis. Validate bounded output, cancellation/child cleanup and managed file paths.
3. Minimal database migration and download service integrated into the existing task lifecycle and media import; test pause/retry, provenance, no-overwrite and import failures.
4. Download workspace, module navigation, integrated settings and queue interactions; frontend contract/interaction tests.
5. Tool/license inventory, full regression tests, production build, one fresh final review and delivery report. Report actual results and remaining Windows/source-site manual checks.

## Baseline evidence

- Frontend: 55 files / 267 tests passed outside sandbox.
- Rust: outside sandbox 236 passed, 1 failed, 2 ignored. Existing `creative_auth_failure_is_safe_and_not_retried` requires investigation/recheck; sandbox run's filesystem failures are not product regressions.
- Logs: release-artifacts/v0.21-phase1/. No real website/cookie/account needed for deterministic tests.
