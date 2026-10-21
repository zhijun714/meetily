# Document Storage and Templates Implementation Plan

> **For agentic workers:** Execute independent document-storage and template-management tasks in parallel, then review their shared settings/command integration centrally. The named superpowers execution skills are unavailable in this session; use the available Agent and verification tools.

**Goal:** Automatically save complete transcript TXT and summary Markdown documents to user-selected directories and edit summary templates in Settings.

**Architecture:** Keep SQLite as source of truth; a Rust documents module handles preferences, native directory dialogs, complete database reads, tracked output paths and atomic exports. Extend existing template loader/commands with validated custom CRUD and persistent defaults; React settings forms invoke these commands and emit refresh events.

**Tech Stack:** Existing Rust/Tauri 2, SQLx SQLite, tauri-plugin-store/dialog, Next.js/React/TypeScript and Bun tests.

## Global Constraints
- Scope is transcript documents and summary documents, not audio recording locations.
- No local dependency installation or full Rust/Next build.
- Independent TXT and Markdown export toggles default disabled for existing installations.
- SQLite remains source of truth; external document errors never undo committed meetings/summaries.
- Preserve existing templates, audio behavior, credentials and unrelated untracked .agents/ files.
- Commit/push enhance/custom-transcription and package on GitHub runner.

---

### Task 1: Document storage backend and settings
Files: create `frontend/src-tauri/src/documents/` modules and tests; create `frontend/src/components/DocumentSettings.tsx`; modify API save/import/retranscription/summary completion and edit hooks; add migration for output ownership if needed.
Interfaces: Tauri commands to get/save document preferences, select/open a document directory and export a meeting document. Exports use `kind = transcript | summary`; preference fields `transcript_enabled`, `transcript_folder`, `summary_enabled`, `summary_folder`. Service obtains complete committed DB content and emits a result event with meeting ID, kind, success/error and path.
- [ ] Implement tests with temp directories and in-memory SQLite: complete ordered timestamps; filename uniqueness and stable path after rename/directory change; disabled exports; failed writes preserving DB/old file; protected unrelated files; summary Markdown and legacy handling.
- [ ] Implement persistent typed preferences and native dialog commands; reject relative/unwritable folders before saving enabled preferences.
- [ ] Implement serialized atomic document export and output-path ownership tracking. Return export status separately from DB success, including manual retry for existing meetings.
- [ ] Hook after final DB commit in api_save_transcript, import, retranscription, accepted summary completion and accepted manual summary saves. Prevent stale/cancelled results from exporting.
- [ ] Build document settings with independent switches, browse/open controls, load/save error and cancellation handling.
- [ ] Add meeting export/retry controls and notification listener; use native data reads rather than UI transcript pagination.
- [ ] Add UI tests for preference hydration, folder cancel, validation, save failure and export retry.

### Task 2: Template backend and editor
Files: `frontend/src-tauri/src/summary/template_commands.rs`, `templates/loader.rs`, `templates/types.rs`, new management helper/tests; `frontend/src/components/TemplateSettings.tsx`; `frontend/src/hooks/meeting-details/useTemplates.ts`.
Interfaces: commands `api_get_template_editor`, `api_save_template`, `api_delete_template`, `api_get_default_template`, `api_set_default_template`. Existing list API stays compatible, new editor response includes ID/source/full Template. Emit `templates-updated` after mutations/default changes.
- [ ] Test temporary-directory CRUD, ID traversal rejection, whitespace validation, built-in protection, preservation of optional formatting and fallback when default disappears.
- [ ] Expose validated full template documents and source metadata; atomically create/update custom files, delete only custom files, persist default in Tauri store.
- [ ] Make paragraph/list/string affect section instructions used by processor.
- [ ] Implement list plus form editor: new/duplicate/edit custom/delete custom/default, section add/remove/up/down, title/instruction/format/item format and structure preview. Guard unsaved drafts when switching/closing.
- [ ] Refresh useTemplates on events with cleanup and race guards; retain valid explicit choices and initialize saved default.
- [ ] Add focused UI tests for create/save failure, builtin duplication/protection, ordering and unsaved switch handling.

### Task 3: Central integration, validation and delivery
Files: `frontend/src-tauri/src/lib.rs`, `frontend/src/app/settings/page.tsx`, `.github/workflows/build-macos.yml`, user documentation.
- [ ] Register new modules and Tauri commands; add Documents/Templates tabs with horizontal overflow at narrow widths.
- [ ] Update CI commands to run new Bun and Rust filters alongside existing custom transcription checks.
- [ ] Review persisted-output ownership, write ordering, stale generations, form cancellation and existing preference compatibility.
- [ ] Run `git diff --check` and Bun transpiler parsing locally; run dependency-free tests without installing dependencies. Full TypeScript/React/Rust tests execute on CI.
- [ ] Commit explicit changed files with coauthor trailer, push branch, track build. Fix and rerun if needed; deliver successful artifact link.

## Review
Spec coverage includes separate directories/toggles, final-content export, retries, history manual export, stable filenames, template CRUD/default/preview and prompt formats, CI delivery. Audio folder migration, live partial transcript files, Word/PDF export and importing external document edits remain outside scope.
