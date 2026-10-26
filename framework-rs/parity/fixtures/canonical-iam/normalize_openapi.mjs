import assert from 'node:assert/strict';

// Canonical-fixture normalization: resolve local schema refs, sort set-like
// fields and remove schema annotations. Never erase validation/error contracts.
export function normalizeOpenApi(document) {
 function schema(value) {
  if(value===undefined)return undefined;
  if(typeof value==='boolean')return value;
  if(value.$ref){
   assert.match(value.$ref,/^#\/components\/schemas\//);
   const name=value.$ref.slice('#/components/schemas/'.length).replaceAll('~1','/').replaceAll('~0','~');
   assert.ok(document.components.schemas[name],`unresolved schema ${name}`);
   assert.deepEqual(Object.keys(value),['$ref'],'fixture refs have no siblings');
   return schema(document.components.schemas[name]);
  }
  const result={};
  for(const [key,item] of Object.entries(value)){
   if(['title','example','$schema','description'].includes(key))continue;
   if(key==='properties')result[key]=Object.fromEntries(Object.entries(item).map(([name,child])=>[name,schema(child)]));
   else if(key==='items')result[key]=schema(item);
   else if(['oneOf','anyOf','allOf'].includes(key))result[key]=item.map(schema);
   else if(key==='type')result.type=(Array.isArray(item)?item:[item]).sort();
   else if(key==='required')result.required=[...item].sort();
   else result[key]=item;
  }
  // null's singleton enum adds no constraint beyond its type.
  if(JSON.stringify(result.type)==='["null"]'&&JSON.stringify(result.enum)==='[null]')delete result.enum;
  // Only flatten disjoint, unconstrained primitive type alternatives.
  const alternatives=result.oneOf??result.anyOf;
  if(Object.keys(result).length===1&&alternatives?.every(branch=>Object.keys(branch).length===1&&branch.type?.length===1)){
   const types=alternatives.map(branch=>branch.type[0]);
   if(new Set(types).size===types.length&&!types.includes('integer')&&types.every(type=>['string','number','boolean','null'].includes(type)))return {type:types.sort()};
  }
  return result;
 }
 const operations={};
 for(const [path,item] of Object.entries(document.paths))for(const [method,operation] of Object.entries(item)){
  operations[`${method.toUpperCase()} ${path}`]={
   parameters:(operation.parameters??[]).map(parameter=>({name:parameter.name,in:parameter.in,required:parameter.required??false,schema:schema(parameter.schema)})).sort((a,b)=>`${a.in}:${a.name}`.localeCompare(`${b.in}:${b.name}`)),
   security:(operation.security??[]).map(requirement=>Object.entries(requirement).map(([name,scopes])=>{
    const definition=document.components.securitySchemes[name];assert.ok(definition,`unresolved security ${name}`);
    const {bearerFormat,...security}=definition;return {...security,scopes};
   })),
   responses:Object.fromEntries(Object.entries(operation.responses).map(([status,response])=>[status,Object.fromEntries(Object.entries(response.content??{}).map(([media,content])=>[media,schema(content.schema)]))]))
  };
 }
 return sortKeys(operations);
}
export function sortKeys(value){
 if(Array.isArray(value))return value.map(sortKeys);
 if(value&&typeof value==='object')return Object.fromEntries(Object.keys(value).sort().map(key=>[key,sortKeys(value[key])]));
 return value;
}
