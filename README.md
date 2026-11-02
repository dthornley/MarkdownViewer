# Markdown Viewer

A simple, friendly viewer for Markdown files. One HTML file, no build step. Use it from the terminal with the `mdview` command, or install the macOS app and double-click any `.md` file.

## Features

- GitHub-flavored Markdown: tables, task lists, strikethrough, autolinks, footnotes
- GitHub-style alerts (`> [!NOTE]`, `[!TIP]`, `[!WARNING]`, ...)
- Syntax-highlighted code blocks with a copy button
- Mermaid diagrams (flowcharts, sequence, pie, gantt, class, state, ...)
- LaTeX math via KaTeX (`$...$` and `$$...$$`)
- YAML front matter shown in a collapsible block
- Table of contents sidebar with scroll tracking, heading anchors
- Light and dark themes, print / save as PDF
- Works fully offline: all rendering libraries are bundled
- macOS app (Tauri): opens `.md` files from Finder, drag and drop onto the window, live reload
- Drag and drop, file picker, or paste Markdown straight into the page
- Live reload when opened through the `mdview` command (server-sent events, keeps your scroll position)
- Relative links to other `.md` files open inside the viewer; back and forward work

## Usage

### From the terminal (recommended)

```sh
node bin/mdview.js path/to/file.md
```

This starts a tiny local server, opens your browser, and reloads the page the moment the file changes, keeping your scroll position. Relative images resolve correctly, and links to other Markdown files in the same tree open in the viewer.

Install it globally so `mdview file.md` works anywhere:

```sh
npm link
```

### Without a server

Open `index.html` directly in a browser and drop a `.md` file onto it, use **Open…**, or paste Markdown. In Chrome and Edge the **Reload** button re-reads the file from disk.

## Desktop app (macOS)

The same page, wrapped in a small [Tauri](https://tauri.app) shell (`src-tauri/`). It registers as a viewer for `.md`, `.markdown`, `.mdown` and `.mkd`, so files open from Finder, from the Dock icon, by drag and drop onto the window, with **File › Open…** (⌘O), or from the terminal:

```sh
open -a "Markdown Viewer" README.md
```

Edits to the open file show up immediately, relative images resolve against the file, links to other Markdown files open in the app, and external links open in your browser.

**Download:** the latest `.dmg` is on the [Releases page](https://github.com/dthornley/MarkdownViewer/releases) (Apple Silicon).

### Building the app

Requires Node, Rust (`brew install rustup && rustup-init`) and the Xcode command line tools.

```sh
npm install
npm run app:dev           # run the app with debug logging and live rebuilds
npm run dist:unsigned     # ad-hoc signed .app and .dmg in release/ (fine for your own Mac)
npm run dist              # signed + notarized .app, .dmg and .zip using your Apple Developer account
```

`npm run dist` signs with the *Developer ID Application* certificate in the login keychain (discovered
automatically, or set `APPLE_SIGNING_IDENTITY`) and notarizes with the `notarytool` keychain profile named
`MarkdownViewer`. To use a different profile:

```sh
APPLE_KEYCHAIN_PROFILE=MyProfile npm run dist
```

If the profile is missing (new Mac, or first release), create it once. You will be prompted for an
app-specific password from https://account.apple.com (Sign-In and Security › App-Specific Passwords):

```sh
xcrun notarytool store-credentials "MarkdownViewer" --apple-id <your-apple-id> --team-id <team-id>
```

Verify a build with `codesign --verify --deep --strict "release/Markdown Viewer.app"`,
`spctl --assess --type execute "release/Markdown Viewer.app"` (expect `Notarized Developer ID`)
and `xcrun stapler validate release/*.dmg`.

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `r` | Reload file |
| `t` | Toggle table of contents |
| `d` | Toggle dark mode |
| `p` | Print |

## Notes

All rendering libraries (markdown-it, highlight.js, Mermaid, KaTeX, DOMPurify) are bundled in `vendor/`, so the viewer works fully offline. Rendered HTML is sanitized with DOMPurify.

If a library fails to load (for example `index.html` was copied somewhere without its `vendor/` folder), the page shows a clear error listing the failed files instead of staying blank. Only markdown-it is required: when optional libraries are missing the viewer still renders, with a dismissable warning naming the unavailable features, and inline HTML is disabled when DOMPurify is absent.

## Layout

```
index.html            the whole viewer: markup, styles and script
vendor/               bundled rendering libraries (markdown-it, highlight.js, KaTeX, DOMPurify, Mermaid)
bin/mdview.js         CLI: tiny local server with live reload (fs.watch + server-sent events)
src-tauri/            desktop app shell: Rust side (mdv:// file protocol, watcher, menu), config, icons
scripts/stage-frontend.js   copies index.html + vendor/ into dist/ for the app build
scripts/dist.sh       builds, signs, notarizes and packages the macOS app into release/
```
