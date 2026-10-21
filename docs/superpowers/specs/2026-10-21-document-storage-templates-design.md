# Document storage and summary template management

Approved in conversation on 2026-10-21. Scope is transcript documents and summary documents, not audio recording locations.

## Document storage
Settings gains a Documents page with independent enable switches and native folder pickers for transcript TXT and summary Markdown files. Preferences persist across restarts; existing installations default to disabled. Enabling requires a writable absolute directory. SQLite remains the source of truth.

Generate a complete transcript after its database transaction commits, including recording stop/recovery, import and retranscription. Include meeting title/date and relative timestamps. Export a summary only after its current generation or manual edit is committed. Never export cancelled or superseded output. Handle Markdown and supported legacy/block content without overwriting an existing document with empty output.

A shared Rust service owns formatting, serialized atomic writes and stable per-meeting output paths. First export chooses current configured directory and a sanitized filename containing creation date, title and stable meeting ID. Later exports update that document despite title or directory setting changes. Existing files are not moved. Independent toggles control subsequent automatic updates. Manual export/retry from meeting details uses the same service and configured directory. A document failure must not roll back a successful database save; notify the UI and expose retry. Preserve externally modified/unowned destination files rather than blindly overwrite them.

## Templates
Settings gains a Templates page. Users can create a custom template, duplicate any template, edit custom templates, delete custom templates and choose a persisted default. Built-in and bundled templates remain intact. A form edits name, description and ordered sections; each section has title, generation instruction and paragraph/list/string format, with optional item formatting. Preview displays the resulting structure, not a generated model response. Validate blank fields, IDs and formats on the backend; restrict operations to the custom template directory and use atomic writes. Preserve pre-existing custom templates and formatting hints. Saving broadcasts a template change event; open meeting pages refresh without losing a valid explicit selection. New page instances use the saved default, falling back to standard_meeting when unavailable. Format instructions must actually reach the summary prompt. Dirty form changes require an explicit save/discard choice before switching or closing.

## Verification and delivery
No local dependency installation or full Rust/Next build. Use lightweight existing tools for parsing and focused dependency-free checks. CI performs TypeScript, React tests and Rust tests plus unsigned Apple Silicon application packaging. Cover formatting, stable paths, collisions, write failures, stale generation, template validation and CRUD, defaults and UI save behavior. Review integrations, commit and push enhance/custom-transcription, monitor CI and provide the artifact URL.
