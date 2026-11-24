## 1.5.0

- Fixed: with several windows open, a menu command such as Check for Updates… ran in every window (so each showed its own "up to date" dialog), a file change reloaded every window, and a file dropped on or opened into one window could load in the others too. Each window now only handles its own events.

- **Find in page** (⌘F, `f` or `/`, Edit › Find…): every match is highlighted, including ones that span bold, links or code, and ↩ / ⇧↩ (⌘G / ⌘⇧G) step through them. The search follows live reloads.
- **Review mode** (Review button, `c`, View › Review Mode): select text and click Comment to leave a note with an optional suggested replacement; comments are listed in a sidebar, can be resolved, edited and deleted, and are saved beside the document in `<file>.review.json`. They stay anchored to their text as the file changes and the whole review can be copied as Markdown. The `mdview` server stores reviews the same way; the plain page keeps them in the browser.

- New **Markdown Viewer › Install Command Line Tool…** puts an `mdview` command on your PATH, so `mdview README.md` opens files in the app from the terminal. It goes in `/usr/local/bin` (macOS asks for an administrator password), falling back to Homebrew's bin directory, `~/.local/bin` or `~/bin` when that is not on your shell's PATH. Choosing it again rewrites the command.
