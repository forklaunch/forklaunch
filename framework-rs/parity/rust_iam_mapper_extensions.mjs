// Executes actual TS base service classes with custom application mappers.
// The test double records the EntityManager boundary; it is not a database oracle.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const actual=actualValidator(root,spec=>{
  if(spec==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{}})};
  if(spec.includes('/persistence/entities'))return {};
});
const events=[];
function manager(name){return {
 name,rows:new Map(),
 persist(input){events.push([name,'persist']);for(const row of Array.isArray(input)?input:[input])this.rows.set(row.id,row);return this;},
 async flush(){events.push([name,'flush']);},
 async upsertMany(rows){return rows;},
 merge(row){events.push([name,'merge']);return row;},
 async findOneOrFail(entity,filter){events.push([name,'read',entity.name]);const row=this.rows.get(typeof filter==='string'?filter:filter.id);assert.ok(row);return row;},
};}
const fallback=manager('default'),caller=manager('caller'),context={prefix:'custom'},entity={name:'CustomEntity'};
let next=0;
for(const family of ['User','Role','Organization','Permission']){
  const filename=family[0].toLowerCase()+family.slice(1);
  const Service=actual.load(path.join(root,`blueprint/implementations/iam/base/services/${filename}.service.ts`))[`Base${family}Service`];
  const mappers={
    [`${family}Mapper`]:{entity,async toDto(row){events.push(['mapper','project',family]);return {id:row.id,display:row.label};}},
    [`Create${family}Mapper`]:{entity,async toEntity(input,em,extra){assert.equal(extra,context);assert.equal(em,input.expectedManager);events.push(['mapper','create',family,em.name]);return {id:`mapped-${++next}`,label:`${extra.prefix}:${input.name}`};}},
    [`Update${family}Mapper`]:{entity,async toEntity(input,em,extra){assert.equal(extra,family==='Permission'?undefined:context);assert.equal(em,input.expectedManager);events.push(['mapper','update',family,em.name]);return {id:input.id,label:`${(extra??context).prefix}:${input.name}`};}},
  };
  const factory=()=>({getBatchRoles(){throw Error('unexpected role resolution');}});
  const service=family==='User'?new Service(fallback,factory,factory,{},actual.validator,mappers):family==='Permission'?new Service(fallback,factory,{},actual.validator,mappers):new Service(fallback,{},actual.validator,mappers);
  let index=events.length;
  const created=await service[`create${family}`]({name:'Custom',expectedManager:caller},caller,context);
  assert.equal(created.display,'custom:Custom');
  assert.equal(events.slice(index).filter(event=>event[1]==='flush').length,0,'caller controls flush');
  assert.ok(events.slice(index).some(event=>event[0]==='caller'&&event[1]==='persist'));
  const updated=await service[`update${family}`]({id:created.id,name:'Updated',expectedManager:caller},caller,context);
  assert.equal(updated.display,'custom:Updated');
  const read=await service[`get${family}`]({id:created.id},caller);
  assert.equal(read.display,'custom:Updated');
  index=events.length;
  await service[`create${family}`]({name:'Default',expectedManager:fallback},undefined,context);
  assert.equal(events.slice(index).filter(event=>event[0]==='default'&&event[1]==='flush').length,1);
}
// Permission relationship adapters receive the exact caller EM as well.
const {BasePermissionService}=actual.load(path.join(root,'blueprint/implementations/iam/base/services/permission.service.ts'));
const added=[];
const role={id:'related-role',permissions:{add:value=>added.push(value)}};
const relationService=new BasePermissionService(fallback,()=>({async getBatchRoles(ids,em){assert.equal(em,caller);assert.equal(ids.ids[0],role.id);return [role];}}),{},actual.validator,{
 PermissionMapper:{entity,async toDto(row){return {id:row.id};}},
 CreatePermissionMapper:{entity,async toEntity(_input,em,extra){assert.equal(em,caller);assert.equal(extra,context);return {id:'related-permission'};}},
 RoleEntityMapper:{entity,async toEntity(value,em){assert.equal(value,role);assert.equal(em,caller);return value;}},
});
await relationService.createPermission({addToRolesIds:[role.id]},caller,context);
assert.equal(added[0].id,'related-permission');
console.log('PASS actual TS mapper extensions: four custom projections, typed extra context, caller EntityManager identity, caller-owned flush, default flush, permission relationship mapper propagation and TS permission-update extra-context omission; native core entity families remain bounded');
