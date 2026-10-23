import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const cases=[];
function add(value,unsupported=false){cases.push({raw:JSON.stringify(value),unsupported});}
for(const value of [undefined,null,true,false,[],[0],{}, {toString:null},0,-1,0.9,-0.9,1.999,-1.999,8.64e15,-8.64e15,8.64e15+1,-8.64e15-1,1e300,-1e300])add(value);
cases.push({raw:'-0'});
for(const year of ['0000','0001','0004','0100','0400','1600','1700','1900','2000','2024','2025','9999','+010000','-000001','-000004','-000100','-000400','+275760','-271821','+999999','-999999']){
 add(year);
 for(const month of ['00','01','02','04','06','09','12','13']){
  add(`${year}-${month}`,month==='00'||month==='13');
  for(const day of ['00','01','28','29','30','31','32']){
   const date=`${year}-${month}-${day}`;add(date,['00','13'].includes(month)||['00','32'].includes(day));
   for(const suffix of ['T00:00Z','T12:34:56.123Z','T24:00:00.000Z','T00:00:00+23:59','T23:59:59-2359'])add(date+suffix);
  }
 }
}
for(const date of ['2025-01-01','2000-02-29','1900-02-28','+275760-09-13','-271821-04-20']){
 for(const time of ['00:00','23:59','24:00','24:01','25:00','12:60','00:00:00','23:59:59','00:00:60','24:00:01','24:00:00.0','24:00:00.0000','24:00:00.0001','24:00:00.001','00:00:00.1','00:00:00.12','00:00:00.123','00:00:00.123456789','00:00:00.0009']){
  for(const offset of ['Z','+00:00','-0000','+01:30','-0130','+23:59','-23:59','+24:00','-2400','+00:60'])add(`${date}T${time}${offset}`);
 }
}
for(const value of ['-000000','-000000-01-01T00:00:00Z','-000000-01-01T00:00:00+0000','+275760-09-13T00:00:00.001Z','-271821-04-19T23:59:59.999Z'])add(value);
const generatedCount=cases.length;
const outside=['2025-01-01T12:30','2025-01-01T00:00:00','2025-13-01T00:00','-000000-01-01T00:00','2025-01-01t00:00:00z',' 2025-01-01 ','01/02/2025','Jan 2 2025','-000000-01','-000000-01-01','2025-01-01T00:00+€x','2025-01-01T00:00+01','not a date',''];
for(const value of outside)cases.push({raw:JSON.stringify(value),unsupported:true});
function oracle(input){
 const result=validator.parse(statics.date,input);
 return result.ok?{ok:true,millis:result.value.getTime()}:JSON.parse(JSON.stringify(result));
}
const expected=[];
for(const zone of ['UTC','America/Los_Angeles','Asia/Kathmandu']){
 process.env.TZ=zone;
 const results=cases.slice(0,generatedCount).map(value=>value.unsupported?{unsupported:true}:oracle(value.raw===undefined?undefined:JSON.parse(value.raw)));
 if(expected.length){for(let i=0;i<results.length;i++)assert.deepEqual(results[i],expected[i],`timezone ${zone} ${JSON.stringify(cases[i])}`);}else expected.push(...results);
}
for(const value of outside){
 // Record the actual oracle even for unsupported inputs, but do not pretend
 // the restricted native profile implements legacy or timezone-local parsing.
 oracle(value);expected.push({unsupported:true});
}
const supportedCount=cases.filter(value=>!value.unsupported).length;
const unsupportedCount=cases.length-supportedCount;
if(process.env.FORKLAUNCH_DATE_ORACLE_ONLY==='1'){
 console.log(`PASS ${supportedCount} actual source date outcomes in three timezones; ${unsupportedCount} explicit unsupported forms`);
}else{
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','typescript_date_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:32*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let i=0;i<cases.length;i++)assert.deepEqual(results[i],expected[i],JSON.stringify(cases[i]));
 console.log(`PASS ${supportedCount} actual ForkLaunch Zod date outcomes across three timezones and ${unsupportedCount} explicit unsupported forms; TimeClip/calendar/offset/fraction boundaries`);
}
