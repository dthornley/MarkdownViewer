#!/usr/bin/env node
// mdview: open a Markdown file in the viewer with live reload.
// Usage: mdview <file.md> [--port N] [--no-open]
const http = require('http');
const fs = require('fs');
const path = require('path');
const { execFile } = require('child_process');

const args = process.argv.slice(2);
const noOpen = args.includes('--no-open');
const portIdx = args.indexOf('--port');
const port = portIdx !== -1 ? Number(args[portIdx + 1]) : 0;
const target = args.find((a, i) => !a.startsWith('--') && args[i - 1] !== '--port');

if (!target || args.includes('-h') || args.includes('--help')) {
  console.log('Usage: mdview <file.md> [--port N] [--no-open]');
  process.exit(target ? 0 : 1);
}

const filePath = path.resolve(target);
if (!fs.existsSync(filePath)) { console.error(`mdview: no such file: ${filePath}`); process.exit(1); }
const root = path.dirname(filePath);
const viewerHtml = path.join(__dirname, '..', 'index.html');

const MIME = {
  '.html': 'text/html; charset=utf-8', '.md': 'text/markdown; charset=utf-8', '.markdown': 'text/markdown; charset=utf-8',
  '.txt': 'text/plain; charset=utf-8', '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif',
  '.svg': 'image/svg+xml', '.webp': 'image/webp', '.css': 'text/css', '.js': 'text/javascript', '.json': 'application/json',
  '.pdf': 'application/pdf',
};

function safeJoin(base, rel) {
  const p = path.normalize(path.join(base, rel));
  return p.startsWith(base + path.sep) || p === base ? p : null;
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://localhost');
  const pathname = decodeURIComponent(url.pathname);

  if (pathname === '/' || pathname === '/index.html') {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
    return fs.createReadStream(viewerHtml).pipe(res);
  }
  if (pathname === '/__mtime') {
    const p = safeJoin(root, url.searchParams.get('file') || '');
    if (!p || !fs.existsSync(p)) { res.writeHead(404); return res.end(); }
    res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
    return res.end(JSON.stringify({ mtime: fs.statSync(p).mtimeMs }));
  }
  const p = safeJoin(root, pathname);
  if (!p || !fs.existsSync(p) || fs.statSync(p).isDirectory()) { res.writeHead(404); return res.end('Not found'); }
  res.writeHead(200, { 'Content-Type': MIME[path.extname(p).toLowerCase()] || 'application/octet-stream', 'Cache-Control': 'no-store' });
  fs.createReadStream(p).pipe(res);
});

server.listen(port, '127.0.0.1', () => {
  const addr = `http://127.0.0.1:${server.address().port}/?file=${encodeURIComponent(path.basename(filePath))}`;
  console.log(`mdview: serving ${filePath}\n  ${addr}\nPress Ctrl+C to stop.`);
  if (!noOpen) {
    const cmd = process.platform === 'darwin' ? 'open' : process.platform === 'win32' ? 'cmd' : 'xdg-open';
    const cmdArgs = process.platform === 'win32' ? ['/c', 'start', '', addr] : [addr];
    execFile(cmd, cmdArgs, () => {});
  }
});
