#!/usr/bin/env node
// Copies the viewer page and its bundled libraries into dist/ for the Tauri build.
// There is no build step: the app ships index.html exactly as the CLI serves it.
const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..');
const dist = path.join(root, 'dist');
fs.rmSync(dist, { recursive: true, force: true });
fs.mkdirSync(dist, { recursive: true });
fs.copyFileSync(path.join(root, 'index.html'), path.join(dist, 'index.html'));
fs.cpSync(path.join(root, 'vendor'), path.join(dist, 'vendor'), { recursive: true });
console.log(`staged frontend in ${path.relative(process.cwd(), dist) || '.'}`);
