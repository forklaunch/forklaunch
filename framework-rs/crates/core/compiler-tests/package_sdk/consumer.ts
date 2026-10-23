import { createForkLaunchSdk, type Item } from './items';
const item: Item = { name: 'valid' };
async function main() {
 const call = (body: unknown) => createForkLaunchSdk({baseUrl:'http://127.0.0.1',fetch:async()=>new Response(JSON.stringify(body),{headers:{'content-type':'application/json'}})}).items.items.fetch();
 const accepted = await call(item); if(accepted.status!==200 || accepted.response.name!=='valid') throw Error('valid response rejected');
 for (const body of [{name:'x'},{name:4}]) { let denied=false;try { await call(body); } catch { denied=true; } if(!denied) throw Error('invalid response accepted'); }
 console.log('Split TS package: valid value accepted; schema and type violations denied');
}
void main();
