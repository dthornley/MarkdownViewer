# Markdown Viewer

A simple, friendly viewer for Markdown files. One HTML file, no build step.

## Features

- GitHub-flavored Markdown: tables, task lists, strikethrough, autolinks, footnotes
- GitHub-style alerts (`> [!NOTE]`, `[!TIP]`, `[!WARNING]`, ...)
- Syntax-highlighted code blocks with a copy button
- Mermaid diagrams (flowcharts, sequence, pie, gantt, class, state, ...)
- LaTeX math via KaTeX (`$...$` and `$$...$$`)
- YAML front matter shown in a collapsible block
- Table of contents sidebar with scroll tracking, heading anchors
- Light and dark themes, print / save as PDF
- Drag and drop, file picker, or paste Markdown straight into the page
- Live reload when opened through the `mdview` command

## Usage

### From the terminal (recommended)

```sh
node bin/mdview.js path/to/file.md
```

This starts a tiny local server, opens your browser, and reloads the page whenever the file changes. Relative images and links inside the document resolve correctly.

Install it globally so `mdview file.md` works anywhere:

```sh
npm link
```

### Without a server

Open `index.html` directly in a browser and drop a `.md` file onto it, use **Open…**, or paste Markdown. In Chrome and Edge the **Reload** button re-reads the file from disk.

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `r` | Reload file |
| `t` | Toggle table of contents |
| `d` | Toggle dark mode |
| `p` | Print |

## Notes

Rendering libraries (markdown-it, highlight.js, Mermaid, KaTeX, DOMPurify) load from jsDelivr, so an internet connection is needed the first time. Rendered HTML is sanitized with DOMPurify.
