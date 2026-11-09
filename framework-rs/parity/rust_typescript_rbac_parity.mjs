// Compare session authorization only; credential verification has separate fixtures.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load,validator}=actualValidator(root,(specifier,file)=>{
 if(specifier.endsWith('/discriminateAuthMethod'))return {discriminateAuthMethod:async config=>({type:'jwt',auth:{decodeResource:config.decodeResource}})};
 if(specifier.endsWith('/authLogContext'))return {authLogContext:()=>({}),jwtFailureReason:()=>''};
 if(specifier.endsWith('/pinoLogger'))return {meta:value=>value};
 if(specifier==='@forklaunch/common'&&file.endsWith('auth.middleware.ts'))return {isNever:()=>{throw Error('unexpected auth mechanism');}};
});
const {parseRequestAuth}=load(path.join(root,'framework/core/src/http/middleware/request/auth.middleware.ts'));
const policies=[
 {},{allowedRoles:['admin']},{forbiddenRoles:['blocked']},
 {allowedRoles:[],forbiddenRoles:['blocked']},{allowedRoles:[]},{forbiddenRoles:[]},
 {allowedPermissions:['read']},{forbiddenPermissions:['banned']},
 {allowedPermissions:[],forbiddenPermissions:['banned']},{allowedPermissions:[]},
 {allowedPermissions:['read'],allowedRoles:['admin']},
 {forbiddenPermissions:[],allowedRoles:['admin']},
 {allowedPermissions:['read'],forbiddenPermissions:['banned'],forbiddenRoles:['blocked']},
 {requiredScope:'team',scopeHeirarchy:['admin','org','team']},
 {requiredScope:'team'},{requiredScope:'team',scopeHeirarchy:[]},
 {requiredScope:'',scopeHeirarchy:[]},
 {requiredScope:'unknown',scopeHeirarchy:['admin','org','team']},
 {requiredFeatures:['one','two']},{requiredFeatures:[]},{requireActiveSubscription:true},
 {requireActiveSubscription:true,requiredFeatures:['one','two'],allowedRoles:['admin']},
 {requiredScope:'team',scopeHeirarchy:['admin','org','team'],allowedPermissions:['read'],requiredFeatures:['one'],requireActiveSubscription:true}
];
const principals=[
 {},{roles:['admin']},{roles:['blocked','admin'],permissions:['read']},
 {permissions:['read']},{permissions:['read','banned']},
 {scopes:['admin']},{scopes:['team']},{scopes:['team','unknown']},
 {scopes:['org','team'],features:['one','two'],active_subscription:true},
 {features:['one']},{features:['one','two']},{active_subscription:true},
 {roles:['admin'],permissions:['read'],scopes:['admin','org','team'],features:['one','two'],active_subscription:true}
].map(value=>({subject:'verified-subject',roles:[],permissions:[],scopes:[],features:[],active_subscription:false,...value}));
const fixtures=[],expected=[];
for(const access of [null,'public','authenticated','protected','internal'])for(const policy of policies)for(const principal of principals) {
 fixtures.push({access,policy,principal});
 const auth={jwt:{signatureKey:'unused-in-session-only-fixture'},decodeResource:async()=>principal,
  surfacePermissions:async()=>new Set(principal.permissions),surfaceRoles:async()=>new Set(principal.roles),
  surfaceScopes:async()=>new Set(principal.scopes),surfaceFeatures:async()=>new Set(principal.features),
  surfaceSubscription:async()=>principal.active_subscription,...policy};
 for(const field of ['allowedPermissions','forbiddenPermissions','allowedRoles','forbiddenRoles'])if(field in policy)auth[field]=new Set(policy[field]);
 const req={method:'GET',path:'/authorization',headers:{authorization:'Bearer verified'},body:undefined,schemaValidator:validator,contractDetails:{auth,...(access===null?{}:{access})}};
 let status=200,message=null,passed=false;
 const res={type(){return this;},status(code){status=code;return this;},send(value){message=value;return this;}};
 await parseRequestAuth(req,res,()=>{passed=true;});
 assert.equal(passed,status===200);
 expected.push({status,message});
}
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','auth','--example','typescript_rbac_parity'],{cwd:root,input:JSON.stringify(fixtures),encoding:'utf8',timeout:120000,env:{...process.env,CARGO_INCREMENTAL:'0'},maxBuffer:16*1024*1024}));
assert.equal(actual.length,fixtures.length);
for(const [index,result] of actual.entries())assert.deepEqual(result,expected[index],JSON.stringify(fixtures[index]));
console.log(`PASS ${fixtures.length} actual TypeScript/native session-authorization comparisons: access, set presence, precedence, scopes, features and subscriptions`);
