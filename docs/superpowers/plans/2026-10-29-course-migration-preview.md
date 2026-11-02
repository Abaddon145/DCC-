# 映流课程迁入冲突预检 Implementation Plan

> **For agentic workers:** Use `executing-plans` inline. This plan covers only a read-only internal preview; source authorization, production SQLite access, user-facing import, persistence, and applying decisions require subsequent work.

**Goal:** Detect loss of course context or progress before mapping verified source items to existing target media, using isolated databases and explicit media bindings.

**Architecture:** Read source series, items, and progress from an already-created consistent schema-8 snapshot; read target courses and memberships from the target connection. A caller supplies explicit source-item-to-target-media bindings after separately verifying media identity. Produce a deterministic conflict report without writing either database; never infer identity from paths, names, or manifest hints.

**Tech Stack:** Rust, existing rusqlite and serde; no new dependency or schema migration.

**Spec:** `docs/superpowers/plans/2026-10-21-stackdcc-yingliu-merge.md` Task 12; `C:/Users/14582/Documents/ChatGPT/视频下载软件/docs/integration/yingliu-migration-input-v1.md` sections 1, 2, 4 and 5.

## Constraints

- Preserve source item IDs and series membership and order; never silently coalesce different source items.
- Target `learning_courses.media_entry_id` is unique and a target series cannot contain the same course twice. Report either collision instead of claiming it can be imported.
- Keep absent progress distinct from a zero position. Preserve source values, target values and target progress revision. Differing progress is a conflict, never choose the maximum.
- Missing or invalid media bindings remain unresolved; no title, path, duration, or hash supplied in a source row establishes target identity.
- No real user database, Vault, media, credentials, paid AI, install, push, merge, or release. Do not change the existing Markdown and external-video archive flows.

## Review Focus

1. Two source items in the same series bound to one target media: report the unrepresentable repeated membership, including both source IDs.
2. Two source items sharing media across series with divergent progress: report all known values; no silent merge.
3. Missing source progress versus an explicit zero and a target already edited: retain the distinction and target revision.
4. Missing, deleted, non-video, duplicate, or unknown bindings: reject malformed input or report unresolved; never write target data.
5. Source snapshot with broken references, invalid progress, or unsupported schema: fail closed; output must not assert a ready import.

## Task 1: Internal course conflict preview

**Files:** Create `src-tauri/src/learning/import_course_preview.rs` and `import_course_preview_tests.rs`; register them in `src-tauri/src/learning/mod.rs`. Reuse the schema-8 synthetic fixture and existing target schema. Do not expose a Tauri command.

**Interfaces:** `preview_courses(source: &Connection, target: &Connection, bindings: &[CourseMediaBinding]) -> Result<CourseImportPreview, String>`. `CourseMediaBinding` contains a source item ID and an already verified target media ID. The report contains each source series ID, title and ordered source items, each item's explicit binding or unresolved status, optional source progress, optional existing target course/progress/revision, and conflict records containing the affected IDs. Its documented return value is a diagnostic preview, not an authorization or a durable import plan.

- [ ] Write tests against the copied synthetic source fixture and an isolated target database. Assert stable series/item ordering by `(sort_order, id)`, distinct source IDs, target membership collision, cross-series divergent progress, absent-versus-zero progress, an existing target revision, unresolved binding, and no changes to source or target rows. Include duplicate and unknown binding IDs, deleted/non-video media, invalid rates or negative times, orphan references, and unsupported schema.
- [ ] Run `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib learning_import_course_preview -- --test-threads=1`; verify the behavioral tests fail before implementation. Tests must use temporary fixture copies and temporary target libraries.
- [ ] Implement fixed, explicit-column queries. Check both schema version markers are 8; reject invalid IDs, orphan series/progress rows, invalid progress values and duplicate bindings. Treat invalid or unavailable target media as unresolved. Group only by verified target media ID and report collisions; retain all source values. Use a target read transaction so membership and revision are from the same view.
- [ ] Run the targeted command, then `node scripts/qa/verify-dcc-learning-migration.mjs`, full Rust and frontend tests, `npm run build`, `cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check`, and `git diff --check`. Use a workspace-contained temporary directory. On Windows sandbox hard-link denial, run the isolated hard-link checks with normal filesystem permissions and report both environments separately.
- [ ] Record actual test counts, exact unimplemented boundaries, and any failures in `docs/evidence/merge/`; review the implementation against the five risks above. Preserve unrelated untracked files and the pre-existing Windows test assertion fix.

## Explicit next gates

This preview alone does not select or open a real source database, verify media bindings, create a series or course, preserve a user's resolution, or migrate progress. Production work still requires authorized source selection and safe DB/WAL/SHM handling; stale-source and stale-target checks at submission; durable source mappings and conflict decisions; transactional and restart-safe apply/revert; user-visible review of all unrepresentable course contexts; and separate approval before any real user-data migration. A successful preview must not be presented as proof of those capabilities.
