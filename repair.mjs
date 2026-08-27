import { generateHmacAuthHeaders } from '@forklaunch/core/http';
import { execFileSync } from 'node:child_process';
const API='http://localhost:8020'; const SECRET=process.env.HMAC_SECRET_KEY;
async function call(m,route,sp,b){const pl=b===undefined?undefined:JSON.stringify(b);
 const {authorization}=generateHmacAuthHeaders({secretKey:SECRET,method:m,path:sp,body:pl});
 const r=await fetch(API+route,{method:m,headers:{'Content-Type':'application/json',Authorization:authorization},...(pl?{body:pl}:{})});
 const t=await r.text(); if(!r.ok) throw new Error(`${m} ${route} ${r.status} ${t.slice(0,150)}`); return t?JSON.parse(t):null;}
const sql=q=>execFileSync('psql',['-d','fl_merch_demo','-tAc',q]).toString().trim();

const FIX=[{h:'tee-flame',t:'Flame Tee',p:3200},{h:'tee-mono',t:'Monospace Tee',p:3200}];
for (const it of FIX){
  const pid=sql(`select id from product where handle='${it.h}'`);
  const have=new Set(sql(`select coalesce(option_values->>'Size','') from variant where product_id='${pid}'`).split('\n').filter(Boolean));
  for (const o of ['XS','S','M','L','XL','2XL']){
    if (have.has(o)) { console.log(`  = ${it.h} ${o} exists`); continue; }
    const v=await call('POST','/variant','/',{productId:pid,
      externalId:`fl-${it.h}-${o.toLowerCase()}`,title:`${it.t} — ${o}`,
      sku:`${it.h}-${o.toLowerCase()}`,priceCents:it.p,currency:'usd',
      optionValues:{Size:o},requiresShipping:true});
    const per=7+(o==='M'||o==='L'?8:0);
    sql(`insert into inventory (id,created_at,updated_at,variant_id,stock) values (gen_random_uuid(),now(),now(),'${v.id}',${per})`);
    console.log(`  + ${it.h} ${o}`);
  }
}
