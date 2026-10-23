import {mkdtempSync, rmSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const directory = mkdtempSync(join(tmpdir(), 'fl-multipart-text-'));
try {
  const env = {...process.env, CARGO_INCREMENTAL:'0', CARGO_BUILD_JOBS:'1', FORKLAUNCH_MULTIPART_REFERENCE_OUTPUT:join(directory,'actual.json')};
  execFileSync(process.execPath,[join(root,'framework-rs/parity/fixtures/rust_multipart_text_reference.mjs')],{env,stdio:'inherit',timeout:120000});
  execFileSync('cargo',['test','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','-p','forklaunch-axum','--all-features','busboy_','--','--nocapture'],{env,stdio:'inherit',timeout:600000});
  console.log('PASS 45 actual multipart text rows in both native parsers, binary preservation, defaults, bounds and version selection identity; mixed file/text chunk-order and JavaScript-only string/undefined identity remain outside this profile');
} finally {
  rmSync(directory,{recursive:true,force:true});
}
