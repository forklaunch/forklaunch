import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics,load}=actualValidator(root);
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES??'/private/tmp/fl-canonical-http-dependencies','package.json'));
const Ajv=require('ajv'),addFormats=require('ajv-formats'),{z}=require('zod/v3');
const ajv=new Ajv({coerceTypes:true,allErrors:true,strict:false});addFormats(ajv);
const email=statics.email;
const variants=[
 ['direct',email,value=>value],
 ['optional',email.optional(),value=>value],
 ['nullable',email.nullable(),value=>value],
 ['object',z.object({email}),value=>({email:value})],
 ['array',email.array(),value=>[value]],
 ['union',z.union([email,z.literal('other')]),value=>value],
 ['nested',z.object({rows:z.array(z.object({email:email.optional()}))}),value=>({rows:[{email:value}]})],
];
const values=["a'b@b.co",'a%b@b.co','a@b-.co','a@b.co\n','a@b.co\r','a@b.co\u2028','a@b.co\u2029','a@b.co','a@B.CO',"a'@b.co",'.a@b.co','a..b@b.co','a@-b.co','a@b.c','é@b.co','a@K.co','a@ſ.co','', 'other'];
for(const base of ['a@b.co',"a'b@b-.co"])for(let pos=0;pos<=base.length;pos++)for(const ch of ['a','Z','0','-',"'",'.','@','%','+','\n','\r','💡','\0',' '])values.push(base.slice(0,pos)+ch+base.slice(pos));
let count=0;
for(const [name,schema,wrap] of variants){
 const exported=validator.openapi(schema),validate=ajv.compile(exported);
 assert.ok(!JSON.stringify(exported).includes('"format":"email"'),name);
 for(const value of values){const input=wrap(value);assert.equal(validate(input),validator.parse(schema,input).ok,JSON.stringify({name,input,exported}));count++;}
}
assert.equal(validator.openapi(z.string().email()).format,'email');
assert.equal(validator.openapi(email.min(1)).format,undefined);
assert.equal(validator.openapi(email.uuid()).format,'uuid');
const overridden=validator.openapi(email.openapi({format:'email',pattern:'^custom$'}));
assert.equal(overridden.format,'email');assert.equal(overridden.pattern,'^custom$');
const typebox=load(path.join(root,'framework/validator/src/typebox/staticSchemaValidator.ts'));
const typeboxExport=typebox.SchemaValidator().openapi(typebox.email);
assert.equal(typeboxExport.pattern,typebox.email.pattern);
assert.equal(typeboxExport.format,undefined);
assert.equal(ajv.compile(typeboxExport)('a%b@b.co'),true);
let callbacks=0;
const callback=()=>{callbacks++;throw Error('must not run during input export');};
for(const schema of [email.transform(callback),email.refine(callback),z.preprocess(callback,email)])validator.openapi(schema);
validator.openapiRequestBody(email.default(callback));
assert.equal(callbacks,0);
assert.equal(validator.openapi(email).format,undefined);
assert.equal(Object.getOwnPropertySymbols(validator.openapi(email)).length,0);
console.log(`PASS ${count} actual Zod factory email parse/export/AJV comparisons across seven direct/nested shapes; unmarked/TypeBox/multicheck/explicit override controls and zero callback execution`);
