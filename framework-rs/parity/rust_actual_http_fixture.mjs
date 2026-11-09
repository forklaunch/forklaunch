// Executes actual repository HTTP clients/signers with a pinned TypeScript compiler.
import {createRequire} from 'node:module';
import {readFileSync,realpathSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import vm from 'node:vm';
export function actualHttpSources(root) {
 const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const loaded=require(require.resolve('typescript',{paths:[path.dirname(realpathSync(tsc))]}));
const ts=loaded.default??loaded,modules=new Map();
function transpile(file){
 if(typeof ts.transpileModule==='function')return ts.transpileModule(readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 // TypeScript 7's public package root is version metadata, not the old compiler
 // API. Use its real compiler executable; no substitute signing implementation.
 const temporary=mkdtempSync(path.join(tmpdir(),'forklaunch-ts-http-'));
 try{
  const compiled=spawnSync(tsc,['--ignoreConfig','--noCheck','--noResolve','--target','ES2022','--module','commonjs','--outDir',temporary,file],{encoding:'utf8'});
  if(compiled.status!==0)throw new Error(`TypeScript fixture compilation failed: ${compiled.stderr||compiled.stdout}`);
  return readFileSync(path.join(temporary,path.basename(file,'.ts')+'.js'),'utf8');
 }finally{rmSync(temporary,{recursive:true,force:true});}
}
function load(file){
 if(modules.has(file))return modules.get(file);
 const exports={};modules.set(file,exports);
 const source=transpile(file).replaceAll('import.meta.url',JSON.stringify(pathToFileURL(file).href));
 vm.runInNewContext(source,{exports,__filename:file,Buffer,Date,URL,Headers,Response,TextDecoder,process,fetch,require(spec){
  if(spec==='@forklaunch/common')return load(path.join(root,'framework/common/src/safeStringify.ts'));
  if(spec.startsWith('.'))return load(path.resolve(path.dirname(file),spec+'.ts'));
  return require(spec);
 }});return exports;
}
return {load};
}
