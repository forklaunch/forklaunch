#!/usr/bin/env node
/**
 * assist-server — local receiver for assisted (real-browser) capture.
 *
 *   node assist-server.mjs <capture-dir> [port=8777]
 *
 * The transfer half of assisted capture (see SKILL.md "When a store won't
 * capture"): the operator's Chrome page POSTs its own rendered DOM here —
 *
 *   fetch('http://127.0.0.1:8777/save?url='+encodeURIComponent(location.href),
 *         { method:'POST', body:'<!DOCTYPE html>'+document.documentElement.outerHTML })
 *
 * — and the page lands in <capture-dir>/.raw/<file> exactly where the
 * crawler checkpoints its own captures (same urlmap mapping, same guard
 * rails as import-dom.mjs), so a resumed `crawl.js --complete` run and
 * check-complete.mjs treat it as captured. Binds 127.0.0.1 only; Chrome
 * permits https->http://localhost posts (localhost is a trustworthy origin).
 */
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
const { pageFileFor } = createRequire(import.meta.url)('./urlmap.js');

const [dir, portArg] = process.argv.slice(2);
if (!dir) { console.error('usage: node assist-server.mjs <capture-dir> [port]'); process.exit(1); }
const PORT = Number(portArg) || 8777;

const server = http.createServer((req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
  if (req.method === 'OPTIONS') { res.writeHead(204); return res.end(); }
  const u = new URL(req.url, 'http://localhost');
  if (req.method !== 'POST' || u.pathname !== '/save') { res.writeHead(404); return res.end('not found'); }

  let pagePath;
  try { pagePath = new URL(u.searchParams.get('url')).pathname; }
  catch { pagePath = u.searchParams.get('url') || ''; }
  if (!pagePath.startsWith('/')) pagePath = '/' + pagePath;
  const mapped = pageFileFor(pagePath);
  if (!mapped) { res.writeHead(422); return res.end(`unmapped path ${pagePath}`); }

  const chunks = [];
  let bytes = 0;
  req.on('data', (c) => { bytes += c.length; if (bytes > 30 * 1024 * 1024) req.destroy(); else chunks.push(c); });
  req.on('end', () => {
    const html = Buffer.concat(chunks).toString('utf8');
    if (html.length < 500 || !/<html[\s>]/i.test(html)) { res.writeHead(422); return res.end('not a rendered HTML document'); }
    const rawFile = path.join(dir, '.raw', mapped.file);
    fs.mkdirSync(path.dirname(rawFile), { recursive: true });
    fs.writeFileSync(rawFile, html);
    console.log(`saved ${pagePath} -> .raw/${mapped.file} (${html.length} bytes)`);
    res.writeHead(200); res.end(`saved .raw/${mapped.file}`);
  });
});
server.listen(PORT, '127.0.0.1', () => console.log(`assist-server on http://127.0.0.1:${PORT}/save?url=... -> ${dir}/.raw`));
