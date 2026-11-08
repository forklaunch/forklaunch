#!/usr/bin/env node
/**
 * import-folder — ingest a folder of bookmarklet-saved pages into a capture.
 *
 *   node import-folder.mjs <capture-dir> <downloads-dir> [--move]
 *
 * Reads every `flcap*.html` the assist bookmarklet dropped in <downloads-dir>
 * (default: the OS Downloads folder), recovers each page's path from its
 * filename ("flcap__products__x.html" -> "/products/x"), and files it into
 * <capture-dir>/.raw/<file> — the same checkpoint the crawler writes, so a
 * resumed `crawl.js --complete` run + check-complete.mjs treat them as
 * captured. --move deletes the source files after a successful import.
 *
 * The completion loop for a walled store:
 *   crawl … --complete            # get what the crawler politely can
 *   check-complete …              # see what's missing
 *   (operator clicks the bookmarklet on each missing page in their browser)
 *   node import-folder.mjs <dir>  # ingest the downloads
 *   crawl … --complete            # resume: rewrite runs over imported pages
 *   check-complete …              # repeat until green
 */
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { createRequire } from 'node:module';
const { pageFileFor } = createRequire(import.meta.url)('./urlmap.js');

const args = process.argv.slice(2);
const move = args.includes('--move');
const [dir, dlArg] = args.filter((a) => a !== '--move');
if (!dir) {
  console.error('usage: node import-folder.mjs <capture-dir> [downloads-dir] [--move]');
  process.exit(1);
}
const dl = dlArg || path.join(os.homedir(), 'Downloads');

const files = fs.readdirSync(dl).filter((f) => /^flcap.*\.html$/i.test(f));
if (!files.length) {
  console.error(`no flcap*.html files in ${dl} — click the bookmarklet on the missing pages first`);
  process.exit(2);
}

let ok = 0, skipped = 0;
for (const f of files) {
  // "flcap__products__x.html" -> "/products/x"; "flcap__index.html" -> "/"
  let p = f.replace(/^flcap/, '').replace(/\.html$/i, '').replace(/__/g, '/');
  if (p === '/index' || p === '') p = '/';
  if (!p.startsWith('/')) p = '/' + p;
  const mapped = pageFileFor(p);
  const src = path.join(dl, f);
  const html = fs.readFileSync(src, 'utf8');
  if (!mapped || html.length < 500 || !/<html[\s>]/i.test(html)) {
    console.error(`  skip ${f} (${!mapped ? 'unmapped path ' + p : 'not a rendered document'})`);
    skipped++;
    continue;
  }
  const rawFile = path.join(dir, '.raw', mapped.file);
  fs.mkdirSync(path.dirname(rawFile), { recursive: true });
  fs.writeFileSync(rawFile, html);
  if (move) fs.rmSync(src);
  console.log(`  ${p} -> .raw/${mapped.file} (${html.length} bytes)`);
  ok++;
}
console.log(`imported ${ok}, skipped ${skipped}`);

// Imported pages land in .raw/ (the checkpoint store the crawler resumes from),
// not in site/ (what actually gets served and what check-complete grades). So
// an import on its own leaves the fidelity gate exactly where it was, which
// reads as "the import silently did nothing" unless you know the rewrite is a
// separate phase. Say so here rather than letting the next gate run look like a
// failure.
if (ok > 0) {
  const domainGuess = path.basename(path.resolve(dir)).replace(/^(cc_|complete_)/, '');
  console.log(
    '\nNext: resume the capture so these are rewritten into site/, then re-check:\n' +
      `  node scripts/crawl.js <domain> ${dir} --complete --clean\n` +
      `  node scripts/check-complete.mjs ${dir} <domain>\n` +
      `(the resume re-fetches nothing that is already checkpointed — for ${domainGuess} it` +
      ' will report them as "already captured")'
  );
}
