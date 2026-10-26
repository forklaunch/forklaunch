// Full source checking of the configured mapper/service roots; no ambient stubs.
import assert from 'node:assert/strict';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
import {readFile,access,mkdtemp,writeFile,rm} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const readJson=async file=>JSON.parse(await readFile(file,'utf8'));
const prefix=process.env.FORKLAUNCH_CONFIGURED_TYPES_PREFIX;
assert.ok(prefix,'Set FORKLAUNCH_CONFIGURED_TYPES_PREFIX to the prefix installed from fixtures/configured-mappers-types/package{,-lock}.json');
const runtimePrefixes=JSON.parse(process.env.FORKLAUNCH_CONFIGURED_TYPES_RUNTIME_PREFIXES??'[]');
assert.ok(Array.isArray(runtimePrefixes)&&runtimePrefixes.length&&runtimePrefixes.every(value=>typeof value==='string'),'Set FORKLAUNCH_CONFIGURED_TYPES_RUNTIME_PREFIXES to a JSON array of installed runtime dependency prefixes');
const modules=path.resolve(prefix,'node_modules');
const dependencyRoots=[modules,...runtimePrefixes.map(value=>path.resolve(value,'node_modules'))];
const fixture=await readJson(path.join(root,'framework-rs/parity/fixtures/configured-mappers-types/package.json'));
const framework=await readJson(path.join(root,'framework/package.json'));
const express=await readJson(path.join(root,'framework/express/package.json'));
for(const [name,version] of Object.entries(fixture.dependencies)){
 assert.equal((express.devDependencies[name]??framework.devDependencies[name]).replace(/^[~^]/,''),version,`${name} fixture must track repository declaration`);
 assert.equal((await readJson(path.join(modules,name,'package.json'))).version,version,`${name} must use the pinned fixture version`);
}
const inventory=await readJson(path.join(root,'plan/rust-typescript-public-api.json'));
const paths={};let entries=0;
for(const pkg of inventory.packages)for(const entry of pkg.entries){
 if(!entry.source)continue;
 const source=path.resolve(root,entry.source);
 assert.ok(source.startsWith(root+path.sep),'inventory source stays in repository');await access(source);
 const specifier=pkg.name+(entry.subpath==='.'?'':entry.subpath.slice(1));
 if(paths[specifier])assert.deepEqual(paths[specifier],[source],`duplicate inventory export ${specifier}`);
 paths[specifier]=[source];entries++;
}
// Private blueprint-core uses legacy `types`, not an exports map; the inventory
// intentionally has no entries for it. Resolve that declared root to real source.
const corePackage=inventory.packages.find(pkg=>pkg.name==='@forklaunch/blueprint-core');
assert.ok(corePackage);
const coreManifest=await readJson(path.join(root,corePackage.manifest));
assert.equal(coreManifest.types,'lib/index.d.ts');
const coreSource=path.join(path.dirname(corePackage.manifest),'index.ts');
assert.ok(corePackage.moduleSources.includes(coreSource));
await access(path.join(root,coreSource));paths[corePackage.name]=[path.join(root,coreSource)];
for(const name of ['body-parser','busboy','cors','express','qs','range-parser','send','swagger-ui-express']){
 const declaration=path.join(modules,'@types',name,'index.d.ts');await access(declaration);paths[name]=[declaration];
}
paths['*']=dependencyRoots.map(directory=>path.join(directory,'*'));
const sdkProfile=process.argv.includes('--sdk');
assert.ok(process.argv.slice(2).every(argument=>argument==='--sdk'),'Only --sdk is supported');
const roots=sdkProfile?['framework/universal-sdk/index.ts']:[
 'blueprint/implementations/billing/stripe/services/plan.service.ts',
 'blueprint/implementations/billing/stripe/services/subscription.service.ts',
 'blueprint/implementations/billing/stripe/services/paymentLink.service.ts',
 'blueprint/implementations/billing/stripe/services/checkoutSession.service.ts',
 'blueprint/implementations/billing/stripe/services/billingPortal.service.ts',
 'blueprint/implementations/ecommerce/stripe/services/payment.service.ts',
 'blueprint/billing-stripe/domain/mappers/plan.mappers.ts',
 'blueprint/billing-stripe/domain/mappers/subscription.mappers.ts',
 'blueprint/billing-stripe/domain/mappers/paymentLink.mappers.ts',
 'blueprint/billing-stripe/domain/mappers/checkoutSession.mappers.ts',
 'blueprint/billing-stripe/domain/mappers/billingPortal.mappers.ts',
 'blueprint/ecommerce-stripe/domain/mappers/payment.mappers.ts'
];
const temporary=await mkdtemp(path.join(os.tmpdir(),'fl-configured-mappers-types-'));
try{
 const config=path.join(temporary,'tsconfig.json');
 await writeFile(config,JSON.stringify({compilerOptions:{target:'ES2022',module:'ESNext',moduleResolution:'Bundler',strict:true,skipLibCheck:true,noEmit:true,experimentalDecorators:true,esModuleInterop:true,lib:['ESNext','DOM'],types:['node','vitest/globals','bun'],typeRoots:[path.join(modules,'@types'),modules],paths},files:roots.map(file=>path.join(root,file))},null,2));
 execFileSync(path.join(modules,'.bin/tsc'),['-p',config,'--pretty','false'],{cwd:root,stdio:'inherit',timeout:120000});
 console.log(`PASS ${sdkProfile?'universal SDK':'configured mapper/service'} TypeScript ${fixture.dependencies.typescript}: all ${roots.length} roots, ${entries} inventory exports, real Node/Bun/Vitest/send/Swagger declarations, zero source diagnostics`);
}finally{await rm(temporary,{recursive:true,force:true});}
