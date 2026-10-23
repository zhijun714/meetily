# Summary Documents Implementation Plan

**Goal:** Consistent dated short document titles, concise readable summaries, and direct Markdown template editing.

**Architecture:** Keep Tauri as the application API. Extend templates with authoritative Markdown while preserving legacy sections. Add isolated naming and conclusion editing helpers; keep document ownership protections in the native exporter.

**Tech Stack:** Rust, SQLx SQLite, Tauri 2, React 18, Next.js 14, TypeScript, Bun.

## Constraints

User approved implementation and push/build on 2026-10-22. Do not stage unrelated .agents or realtime correction design. No archived Python backend changes. Maintain cross-platform paths and cancellation behavior.

## Task 1: Markdown templates

Files: summary/templates/{types,management,loader,defaults}.rs; summary/template_commands.rs; templates/standard_meeting.json; components/TemplateSettings.tsx; lib/template-editor.ts; related Rust/Bun tests.

- [x] Extend Template with optional Markdown body, default legacy fields on deserialize; body wins for generation.
- [x] Render legacy instructions including both formatting hint fields into editable Markdown.
- [x] Allow deliberate save of builtin personal overrides; delete override restores builtin.
- [x] Replace section forms with name + Markdown source editor; retain guards and CRUD. Generate stable custom IDs internally.
- [x] Add regression coverage for Markdown roundtrip, legacy guidance preservation, builtin edit/reset and unsaved draft behavior.

## Task 2: Concise summary and conclusion polish

Files: summary/processor.rs, summary/service.rs, new summary editorial helper and bundled humanizer rules.

- [x] Standard template requests four concise sections and no fabricated assignments.
- [x] After output language transform, extract conclusion using H2 boundaries; apply scoped versioned humanizer rules with the configured provider.
- [x] Validate visible body and preserve other sections exactly; fallback on ordinary failure and propagate cancellation.
- [x] Include policy version in cache identity.
- [ ] Run full processor/cache regression tests including cancellation on macOS CI. Scoped editorial tests pass locally through the external Rust harness.

## Task 3: Naming and native export consistency

Files: summary/service.rs, database repositories meeting/summary, documents/{format,service}.rs, new naming helper/migration if necessary.

- [x] Model returns short title; Rust composes local date plus safe title, max 20 Unicode characters, idempotently.
- [x] Resolve recording start metadata, fallback to saved creation date, persist stable date and manual rename intent as needed.
- [x] Commit final summary/title consistently and preserve user rename during generation.
- [x] Export canonical H1 and filename; safely migrate owned tracked paths while protecting externally modified/unowned files and collisions.
- [x] Verify timezone, repeated generation, user rename, Unicode, tracked rename, collisions and external edits.

## Task 4: Verification and delivery

- [x] Run affected Bun suites, TypeScript validation, production frontend build and native Rust tests/checks appropriate to installed toolchain.
- [x] Review diff for regressions; fix failures and re-run affected checks.
- [ ] Commit task files with Co-Authored-By trailer and push enhance/custom-transcription.
- [ ] Monitor the push-triggered macOS release build until terminal outcome and report run URL/artifact status. GitHub CLI token expired; SSH push and public Actions API remain available.

Local verification: 41 frontend tests passed, TypeScript check and production Next.js build passed. A temporary external Rust crate directly imports the changed native modules and passed 37 tests. Full native app tests require complete Xcode and run in GitHub macOS CI.
