// Actual Node Date reference matrix; does not claim native parser conformance.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {writeFileSync} from 'node:fs';
const dates=new Set(['10/15/2026','2026-10-15T07:23:45','2026-10-15','2026','2026-10','-000000-01-01','0001-01-00','2026-02-31','2026-10-15t07:23:45z','2026-10-15T24:00','2026-10-15T24:01','not-a-date','', '0','1','12','13','49','50','99','100','999','9999']);
for(const day of ['2026-10-15','10/15/2026','15 Oct 2026','October 15, 2026','2026/10/15','2026-1-2','2026-01-02']){
 for(const time of ['', ' 07:23', ' 07:23:45', ' 07:23:45.678', ' 7:23pm',' 7:23 PM',' 07::45']){
  for(const zone of ['', 'Z',' GMT',' UTC',' GMT-0700',' +02:30'])dates.add(day+time+zone);
 }
}
for(const d of ['2026-03-08T01:59:59','2026-03-08T02:00:00','2026-03-08T02:30:00','2026-03-08T03:00:00','2026-11-01T00:59:59','2026-11-01T01:00:00','2026-11-01T01:30:00','2026-11-01T02:00:00','1940-01-01T00:00:00','1900-01-01T00:00:00','0000-01-01T00:00:00','+275760-09-12T23:00:00'])dates.add(d);
for(const d of ['Jan 1 2026','2026-10-15','10/15/2026'])for(const before of ['', 'garbage ', '(comment) ', '\t', '\u00a0'])for(const after of ['', ' ', '(comment)', '(unclosed', '\u0000junk'])dates.add(before+d+after);
const rows=[...dates];const results={};
for(const TZ of ['UTC','America/Los_Angeles','America/New_York','Asia/Kathmandu']){
 results[TZ]=JSON.parse(execFileSync(process.execPath,['--input-type=module','-e',`let input='';for await(const part of process.stdin)input+=part;console.log(JSON.stringify(JSON.parse(input).map(value=>{const time=new Date(value).getTime();return Number.isNaN(time)?null:time;})));`],{env:{...process.env,TZ},input:JSON.stringify(rows),encoding:'utf8'}));
 assert.equal(results[TZ].length,rows.length);
}
const index=rows.indexOf('2026-03-08T02:30:00');assert.equal(results['America/Los_Angeles'][index],Date.parse('2026-03-08T10:30:00Z'));
const fall=rows.indexOf('2026-11-01T01:30:00');assert.equal(results['America/Los_Angeles'][fall],Date.parse('2026-11-01T08:30:00Z'));
const out={node:process.version,v8:process.versions.v8,icu:process.versions.icu,rows,results};if(process.env.FORKLAUNCH_DATE_REFERENCE_OUTPUT)writeFileSync(process.env.FORKLAUNCH_DATE_REFERENCE_OUTPUT,JSON.stringify(out,null,2));
console.log(JSON.stringify({cases:rows.length,zones:Object.keys(results),observations:rows.length*4,admitted:Object.fromEntries(Object.entries(results).map(([tz,values])=>[tz,values.filter(x=>x!==null).length])),timezoneDependent:rows.filter((_,i)=>new Set(Object.values(results).map(v=>v[i])).size>1).length,dstControls:'gap advances, fold chooses earlier'}));
