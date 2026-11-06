## 1.5.0

- New **Markdown Viewer › Install Command Line Tool…** puts an `mdview` command on your PATH, so `mdview README.md` opens files in the app from the terminal. It goes in `/usr/local/bin` (macOS asks for an administrator password), falling back to Homebrew's bin directory, `~/.local/bin` or `~/bin` when that is not on your shell's PATH. Choosing it again rewrites the command.
