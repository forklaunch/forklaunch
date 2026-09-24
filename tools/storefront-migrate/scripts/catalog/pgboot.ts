#!/usr/bin/env bun
/** Init the Postgres scaffold schema and import a normalized catalog into it.
 *   bun scripts/pgboot.ts <pgUrl> <normalized.json> */
import { readFileSync } from 'node:fs';
import { PgStore } from './pgstore.ts';

const [url, normalizedPath] = process.argv.slice(2);
const store = await new PgStore(url).init();
const cat = JSON.parse(readFileSync(normalizedPath, 'utf8'));
const r = await store.importCatalog(cat);
const s = await store.stats();
console.log(`imported ${r.products} products / ${r.variants} variants into Postgres scaffold (now: ${s.products} products, ${s.variants} variants)`);
await store.sql.end();
