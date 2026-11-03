## 1.4.2

- Make Default Markdown Viewer now only claims types that are purely Markdown. Editor-specific types that also cover `.txt`, `.text` or `.note` (Ulysses, iA Writer, MindNode) are skipped, so macOS no longer asks about plain text files.
- A refused editor-specific type is reported as a note once the public Markdown type has been set, instead of as a failure.
