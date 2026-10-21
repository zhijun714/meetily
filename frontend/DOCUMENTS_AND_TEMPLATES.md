# Document folders and summary templates

## Save transcript and summary documents

Open **Settings → Documents**. Transcripts and summaries have independent automatic-save switches and destination folders. Select a folder, enable the desired document type, then save settings.

- Transcripts are UTF-8 TXT documents containing the complete committed transcript and relative timestamps.
- Summaries are UTF-8 Markdown documents, suitable for text editors and Markdown note applications.
- Automatic saves happen after transcript processing completes or a summary is generated/edited and saved in the application. Failed or cancelled summary generations do not replace exported documents.
- Application data remains in SQLite. A folder permission or export error does not discard a successfully saved meeting.
- Meeting details provides document export/retry controls for current or historical meetings. Configure the destination in Documents first.
- The first export chooses a filename containing meeting date, title and a unique meeting identifier. Later saves update its tracked path. Renaming a meeting or changing the configured folder does not move an existing export.
- Turning an automatic-save switch off stops subsequent automatic exports for that document type. It does not delete existing files.
- Files edited outside Meetily are protected from automatic replacement. Keep a separate copy of externally edited documents when you want Meetily to continue maintaining its export.
- When replacing a changed document, Meetily retains its previous file in a hidden `.meetily-document-backup-*` folder next to the export. This also preserves edits from an external editor that still has the old file open. These backups are text documents and can be removed manually after you have checked that no external edits need recovery.

This setting controls transcript and summary documents. Audio recording files and internal recovery JSON files retain their existing storage behavior. Word/PDF export and importing external document edits are outside this feature.

## Edit summary templates

Open **Settings → Templates**. Create a template or duplicate an existing one, then edit its name, description and ordered sections. Each section has a title, instructions and an output format. Optional item-format hints can describe task or table rows. The preview shows the section structure; it does not make a model request.

Built-in templates are read-only. Duplicate them to create an editable custom template. Save before switching templates or leaving the editor; unsaved changes prompt you to save, discard or cancel.

Set a default template for newly opened meeting pages. An explicit template selection on an existing page is preserved while it remains available. Template changes are loaded without rebuilding the application. Generating a summary uses the current saved template; editing a template does not regenerate old summaries automatically.

Custom templates use the existing platform application-data template directory. The editor stores them locally and does not modify application-bundled templates.
