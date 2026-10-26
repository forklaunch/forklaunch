// Actual configured TS surfacing uses literal directory role names / permission
// slugs. Its user-keyed cache differs intentionally from native per-request reads.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const actual=actualValidator(root,specifier=>{
  if(specifier==='@forklaunch/core/http')return {generateHmacAuthHeaders(){throw Error('unexpected network auth');}};
  if(specifier==='@forklaunch/universal-sdk')return {universalSdk(){throw Error('unexpected remote SDK');}};
});
const {createSurfaceRolesLocally,createSurfacePermissionsLocally}=actual.load(path.join(root,'blueprint/iam-base/surfacing.ts'));
const cache=new Map();
const authCacheService={
  getCachedRoles:async id=>cache.get(`roles:${id}`),setCachedRoles:async(id,value)=>cache.set(`roles:${id}`,value),
  getCachedPermissions:async id=>cache.get(`permissions:${id}`),setCachedPermissions:async(id,value)=>cache.set(`permissions:${id}`,value),
};
let roles=[{name:'owner'},{name:'literal role'},{name:'owner'}],permissions=[{slug:'platform:read'},{slug:'opaque/grant'},{slug:'platform:read'}],calls=0;
const userService={
  async surfaceRoles({id}){assert.equal(id,'directory-user');calls++;return roles;},
  async surfacePermissions({id}){assert.equal(id,'directory-user');calls++;return permissions;},
};
const surfaceRoles=createSurfaceRolesLocally({authCacheService,userService});
const surfacePermissions=createSurfacePermissionsLocally({authCacheService,userService});
assert.deepEqual([...await surfaceRoles({})],[]);assert.deepEqual([...await surfacePermissions({})],[]);assert.equal(calls,0);
assert.deepEqual([...await surfaceRoles({sub:'directory-user'})],['owner','literal role']);
assert.deepEqual([...await surfacePermissions({sub:'directory-user'})],['platform:read','opaque/grant']);
roles=[{name:'current-role'}];permissions=[];
assert.deepEqual([...await surfaceRoles({sub:'directory-user'})],['owner','literal role']);
assert.deepEqual([...await surfacePermissions({sub:'directory-user'})],['platform:read','opaque/grant']);
assert.equal(calls,2);
cache.clear();
assert.deepEqual([...await surfaceRoles({sub:'directory-user'})],['current-role']);
assert.deepEqual([...await surfacePermissions({sub:'directory-user'})],[]);
assert.equal(calls,4);
console.log('PASS actual TS directory grant projection: literal names/slugs, set deduplication, missing-sub denial and explicit cache behavior; native stored-session mode intentionally reads fresh');
