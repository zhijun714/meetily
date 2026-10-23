# Summary documents and Markdown templates

Approved by the user on 2026-10-22, including implementation, verification, git push and GitHub build.

- Canonical generated meeting title: yyyyMMdd-shortTitle, local recording start date when available, saved meeting creation date otherwise. Model produces only the short title (prefer 8–16 Chinese characters, maximum 20); formatting is deterministic and idempotent. Preserve manual title edits during regeneration.
- Application title, exported summary H1 and filename stem agree. New exports omit UUIDs and duplicate dates. Collision suffixes are allowed. Rename tracked exports safely on subsequent save/export; preserve externally edited files and ownership checks.
- Standard summary sections: 重点事项, 问题, 待办, 结论. Short substantive points, no repeated executive summary or transcript references. Do not invent owners/deadlines/decisions. Brief empty sections.
- Bundle versioned humanizer-zh editorial rules, including the local plain-language addition. After final language translation, polish only the conclusion with the selected provider. Preserve facts, uncertainty and other sections. Ordinary failure retains original conclusion; cancellation propagates. Local skill changes do not synchronize automatically.
- Template settings: template selector, name and one Markdown source editor. Markdown is authoritative generation guidance, not a projection into hidden per-section forms. Preserve old JSON templates by rendering all legacy instructions to Markdown. Built-in templates editable through personal overrides, with reset to bundled defaults. Retain dirty draft guards, default selection and custom template CRUD. IDs generated internally.
- Changes affect new/regenerated summaries and subsequent exports; no bulk rewrite of historical summaries or external files.
- Validate Rust behavior and frontend UI, inspect final diff, commit only task files, push current feature branch, trigger appropriate GitHub build and report actual outcome.
