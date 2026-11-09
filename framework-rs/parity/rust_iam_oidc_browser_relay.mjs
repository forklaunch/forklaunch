// Real Chromium check of the actual static OIDC relay's cross-site cookie behavior.
// Endpoints are owned HTTPS fixtures; Rust/provider/storage integration runs separately.
import assert from 'node:assert/strict';
import {createServer} from 'node:https';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,readFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

assert.ok(process.env.PLAYWRIGHT_CORE_PATH,'Set PLAYWRIGHT_CORE_PATH to an installed playwright-core directory');
const {chromium}=await import(pathToFileURL(join(resolve(process.env.PLAYWRIGHT_CORE_PATH),'index.mjs')));
const source=new URL('../../blueprint-rs/crates/implementations/iam/iam-base/src/http/browser/',import.meta.url);
const relay=readFileSync(new URL('oidc_relay.html',source),'utf8');
const csp=readFileSync(new URL('oidc_relay.csp',source),'utf8');
const temporary=mkdtempSync(join(tmpdir(),'fl-oidc-browser-'));
const servers=[];
let browser;
try {
 const certificate=spawnSync('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-days','1',
  '-keyout',join(temporary,'key.pem'),'-out',join(temporary,'cert.pem'),'-subj','/CN=localhost'],{encoding:'utf8'});
 assert.equal(certificate.status,0,'Fixture certificate generation failed');
 const tls={key:readFileSync(join(temporary,'key.pem')),cert:readFileSync(join(temporary,'cert.pem'))};
 const seen=[];
 const cookie='__Host-fixture=browser-bound-only';
 let application,identity;
 async function listen(handler){
  const server=createServer(tls,handler);servers.push(server);
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  return server.address().port;
 }
 const appPort=await listen(async(req,res)=>{
  const url=new URL(req.url,application);
  seen.push({path:url.pathname,cookie:req.headers.cookie,referer:req.headers.referer,origin:req.headers.origin});
  res.setHeader('cache-control','no-store');res.setHeader('referrer-policy','no-referrer');
  if(url.pathname==='/begin'){
   res.setHeader('set-cookie',cookie+'; Secure; HttpOnly; SameSite=Strict; Path=/');
   res.setHeader('content-type','text/html');res.end(`<a href="${identity}/authorize">Sign in</a>`);
  }else if(url.pathname==='/auth/browser/oidc/callback'&&req.method==='GET'){
   res.setHeader('content-security-policy',csp);res.setHeader('content-type','text/html; charset=utf-8');res.end(relay);
  }else if(url.pathname==='/auth/browser/csrf'){
   if(req.headers.cookie!==cookie){res.writeHead(401);res.end();return;}
   res.setHeader('content-type','application/json');res.end(JSON.stringify({csrfToken:'owned-proof'}));
  }else if(url.pathname==='/auth/browser/oidc/callback'&&req.method==='POST'){
   let body='';for await(const chunk of req)body+=chunk;
   const valid=req.headers.cookie===cookie&&req.headers.origin===application&&
    req.headers['x-csrf-token']==='owned-proof'&&body===JSON.stringify({code:'owned-code',state:'owned-state'});
   res.writeHead(valid?200:403,{'content-type':'application/json'});res.end('{}');
  }else if(url.pathname==='/'){
   res.setHeader('content-type','text/html');res.end('<p>Signed in</p>');
  }else {res.writeHead(404);res.end();}
 });
 application=`https://127.0.0.1:${appPort}`;
 const providerPort=await listen((req,res)=>{
  if(req.url==='/authorize'){
   res.setHeader('content-type','text/html');res.end('<a href="/complete">Authorize</a>');
  }else {
   res.writeHead(302,{location:`${application}/auth/browser/oidc/callback?code=owned-code&state=owned-state`});res.end();
  }
 });
 // Distinct sites, not merely distinct ports, are required to exercise SameSite.
 identity=`https://localhost:${providerPort}`;
 browser=await chromium.launch({headless:true});
 const context=await browser.newContext({ignoreHTTPSErrors:true});
 const page=await context.newPage();
 await page.goto(`${application}/begin`);
 await page.getByRole('link',{name:'Sign in'}).click();
 await page.getByRole('link',{name:'Authorize'}).click();
 await page.waitForURL(application+'/',{timeout:15000});
 assert.equal(await page.locator('p').textContent(),'Signed in');
 const callback=seen.filter(value=>value.path==='/auth/browser/oidc/callback');
 assert.equal(callback.length,2);
 assert.equal(callback[0].cookie,undefined,'Cross-site callback must not receive Strict cookie');
 assert.equal(callback[1].cookie,cookie,'Same-origin relay POST must retain original browser binding');
 assert.equal(callback[1].origin,application);
 for(const request of seen.filter(value=>['/auth/browser/csrf','/'].includes(value.path))){
  assert.equal(request.referer,undefined,'Code/state must not leak in Referer');
 }
 assert.equal(await page.evaluate(()=>document.cookie),'','Session cookie remains HttpOnly');
 // A direct foreign callback without the initiating cookie must fail visibly.
 const unbound=await browser.newContext({ignoreHTTPSErrors:true});
 const attacker=await unbound.newPage();
 await attacker.goto(identity+'/authorize');
 await attacker.getByRole('link',{name:'Authorize'}).click();
 await attacker.getByText('Sign-in failed. Please start again.').waitFor();
 assert.equal(new URL(attacker.url()).search,'','Relay removes callback secrets even on failure');
 assert.equal(seen.filter(value=>value.path==='/auth/browser/oidc/callback'&&value.origin).length,1);
 await unbound.close();await context.close();
 console.log('PASS real Chromium cross-site OIDC relay, Strict/HttpOnly cookie binding, CSP execution, no-referrer and unbound rejection');
}finally{
 await browser?.close();
 await Promise.all(servers.map(server=>new Promise(resolve=>{server.close(resolve);server.closeAllConnections();})));
 rmSync(temporary,{recursive:true,force:true});
}
