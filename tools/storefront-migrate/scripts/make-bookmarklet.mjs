import fs from 'node:fs'; import path from 'node:path'; import {fileURLToPath} from 'node:url';
const src = fs.readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)),'assist-bookmarklet.js'),'utf8');
// Comments are kept: a text-level strip of `//` also cuts the `/\//g` regex
// literal in the source and leaves the bookmarklet malformed.
const body = src.trim();
console.log('javascript:'+encodeURIComponent(body));
