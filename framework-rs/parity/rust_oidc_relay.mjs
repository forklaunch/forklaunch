import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import vm from 'node:vm';
const root = new URL('../../blueprint-rs/crates/implementations/iam/iam-base/src/http/browser/',import.meta.url);
const html = await readFile(new URL('oidc_relay.html',root),'utf8');
const csp = await readFile(new URL('oidc_relay.csp',root),'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script\b[^>]*>/i)[1];
assert.ok(csp.includes(`script-src 'sha256-${createHash('sha256').update(script).digest('base64')}'`));
for (const query of ['?code=secret-code&state=secret-state','?code=a&code=b&state=x','?error=access_denied&state=x','?code=a&state=']) {
  const events = [];
  const status = {};
  await vm.runInNewContext(script,{
    URLSearchParams,
    location:{search:query,pathname:'/auth/browser/oidc/callback',replace:target=>events.push(['redirect',target])},
    history:{replaceState:(_state,_title,url)=>events.push(['clean',url])},
    document:{getElementById:()=>status},
    fetch:async(url,options)=>{
      events.push(['fetch',url,options]);
      return {ok:true,json:async()=>({csrfToken:'browser-proof'})};
    },
  });
  assert.deepEqual(events[0],['clean','/auth/browser/oidc/callback']);
  if (query.startsWith('?code=secret')) {
    assert.equal(events.length,4);
    assert.equal(events[1][1],'/auth/browser/csrf');
    assert.equal(events[2][1],'/auth/browser/oidc/callback');
    assert.equal(events[2][2].credentials,'same-origin');
    assert.equal(events[2][2].headers['x-csrf-token'],'browser-proof');
    assert.deepEqual(JSON.parse(events[2][2].body),{code:'secret-code',state:'secret-state'});
    assert.deepEqual(events[3],['redirect','/']);
  } else {
    assert.equal(events.length,1);
    assert.equal(status.textContent,'Sign-in failed. Please start again.');
  }
}
console.log('PASS OIDC static relay CSP, URL scrubbing, same-origin callback and rejected query handling (VM; not a browser cookie-policy test)');
