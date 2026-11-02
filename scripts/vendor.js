#!/usr/bin/env node
// Downloads the pinned rendering libraries from the npm registry into vendor/.
// Run `node scripts/vendor.js` after bumping a version below, then re-check sample.md.
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');

// package -> { version, files: { 'path inside the package tarball': 'path under vendor/' } }
const LIBS = {
  'markdown-it': { version: '15.0.3', files: { 'dist/browser/markdown-it.umd.min.js': 'markdown-it.min.js' } },
  'markdown-it-footnote': { version: '4.0.0', files: { 'dist/markdown-it-footnote.min.js': 'markdown-it-footnote.min.js' } },
  'markdown-it-task-lists': { version: '2.1.1', files: { 'dist/markdown-it-task-lists.min.js': 'markdown-it-task-lists.min.js' } },
  'dompurify': { version: '3.4.17', files: { 'dist/purify.min.js': 'purify.min.js' } },
  'mermaid': { version: '12.2.0', files: { 'dist/mermaid.min.js': 'mermaid.min.js' } },
  '@highlightjs/cdn-assets': { version: '11.12.0', files: {
    'highlight.min.js': 'hljs/highlight.min.js', 'styles/github.min.css': 'hljs/github.min.css', 'styles/github-dark.min.css': 'hljs/github-dark.min.css',
  } },
  'katex': { version: '0.19.1', files: {
    'dist/katex.min.js': 'katex/katex.min.js', 'dist/contrib/auto-render.min.js': 'katex/auto-render.min.js', 'dist/katex.min.css': 'katex/katex.min.css',
  }, fonts: 'dist/fonts' },
};

const root = path.join(__dirname, '..');
const vendor = path.join(root, 'vendor');
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mdv-vendor-'));

async function main() {
  fs.rmSync(vendor, { recursive: true, force: true });
  for (const [name, lib] of Object.entries(LIBS)) {
    const meta = await (await fetch(`https://registry.npmjs.org/${name}/${lib.version}`)).json();
    const tarball = path.join(tmp, name.replace('/', '_') + '.tgz');
    fs.writeFileSync(tarball, Buffer.from(await (await fetch(meta.dist.tarball)).arrayBuffer()));
    const dir = path.join(tmp, name.replace('/', '_'));
    fs.mkdirSync(dir, { recursive: true });
    execFileSync('tar', ['xzf', tarball, '-C', dir, '--strip-components=1']);
    for (const [src, dst] of Object.entries(lib.files)) {
      const out = path.join(vendor, dst);
      fs.mkdirSync(path.dirname(out), { recursive: true });
      fs.copyFileSync(path.join(dir, src), out);
    }
    if (lib.fonts) {
      // Modern browsers and WKWebView all speak woff2, so skip the woff/ttf copies (saves ~2 MB)
      // and trim the @font-face rules in katex.min.css to match.
      const fontsOut = path.join(vendor, 'katex/fonts');
      fs.mkdirSync(fontsOut, { recursive: true });
      for (const f of fs.readdirSync(path.join(dir, lib.fonts))) if (f.endsWith('.woff2')) fs.copyFileSync(path.join(dir, lib.fonts, f), path.join(fontsOut, f));
      const css = path.join(vendor, 'katex/katex.min.css');
      fs.writeFileSync(css, fs.readFileSync(css, 'utf8').replace(/,url\(fonts\/[^)]*\.(woff|ttf)\) format\("(woff|truetype)"\)/g, ''));
    }
    console.log(`${name}@${lib.version}`);
  }
  fs.rmSync(tmp, { recursive: true, force: true });
}
main().catch(e => { console.error(e); process.exit(1); });
