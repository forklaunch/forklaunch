import fs from 'node:fs'; import path from 'node:path'; import {fileURLToPath} from 'node:url';
const src = fs.readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)),'assist-bookmarklet.js'),'utf8');
const body = src.replace(/\/\*[\s\S]*?\*\//g,'').replace(/\/\/.*$/gm,'').replace(/\s+/g,' ').trim();
console.log('javascript:'+encodeURIComponent(body));
