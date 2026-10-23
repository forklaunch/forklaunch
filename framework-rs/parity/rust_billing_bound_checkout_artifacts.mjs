// Compile the actual generated billing library template and export both profiles.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const temporary=mkdtempSync(path.join(tmpdir(),'fl-billing-bound-checkout-artifacts-'));
const target=process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target');
const template=readFileSync(path.join(root,'cli/src/templates/rust/billing/api/contracts/registry.rs'),'utf8');
function run(command,args,env={}) {
 const result=spawnSync(command,args,{cwd:temporary,env:{...Object.fromEntries(Object.entries(process.env).filter(([key])=>!['BILLING_BOUND_CHECKOUT_CONFIG','BILLING_BOUND_BILLING_CONFIG','BILLING_PUBLISHER_DATABASE_URL'].includes(key))),CARGO_TARGET_DIR:target,CARGO_INCREMENTAL:'0',...env},encoding:'utf8',timeout:300000,maxBuffer:16*1024*1024});
 assert.equal(result.status,0,result.stderr);return result.stdout;
}
let testedCases=0;
try {
 mkdirSync(path.join(temporary,'src'));
 const crates={
  'forklaunch-core':'framework-rs/crates/core',
  'forklaunch-core':'framework-rs/crates/core',
  'forklaunch-billing-interface':'blueprint-rs/crates/interfaces/billing-interface',
  'forklaunch-billing-stripe':'blueprint-rs/crates/implementations/billing/billing-stripe'
 };
 const dependencies=Object.entries(crates).map(([name,relative])=>`${name} = { path = ${JSON.stringify(path.join(root,relative))}${name==='forklaunch-core'?', features = ["openapi"]':name==='forklaunch-billing-stripe'?', features = ["bound-checkout", "typescript-routing"]':''} }`).join('\n');
 writeFileSync(path.join(temporary,'Cargo.toml'),`[package]\nname="fl-billing-bound-checkout-artifacts"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nserde_json="1"\n${dependencies}`);
 writeFileSync(path.join(temporary,'src/lib.rs'),'mod extensions;\nmod registry;\npub use registry::*;\n');
 writeFileSync(path.join(temporary,'src/extensions.rs'),'pub fn contracts() -> Vec<forklaunch_core::contract::RouteContract> { vec![] }');
 writeFileSync(path.join(temporary,'src/main.rs'),`fn main() {
  let contract=std::path::PathBuf::from(std::env::var("ARTIFACT_DIR").unwrap()).join("contract.json");
  let openapi=contract.with_file_name("openapi.json");
  let result=fl_billing_bound_checkout_artifacts::write_contract_artifacts(&contract,&openapi);
  if std::env::var("EXPECT_FAILURE").as_deref()==Ok("1") {
    assert!(result.is_err()); assert!(!contract.exists()); assert!(!openapi.exists());
  } else { result.unwrap(); }
 }`);
 for(const stripe of [false,true]) {
  const rendered=template.replace(/{{#stripe}}([\s\S]*?){{\/stripe}}/g,(_,body)=>stripe?body:'').replace(/{{\^stripe}}([\s\S]*?){{\/stripe}}/g,(_,body)=>stripe?'':body).replaceAll('{{{name}}}','billing-date-artifacts');
  assert.ok(!rendered.includes('{{'));writeFileSync(path.join(temporary,'src/registry.rs'),rendered);
  run('cargo',['build','--quiet','--manifest-path',path.join(temporary,'Cargo.toml')]);
  const configured={deploymentLabel:'owned',accountId:'acct_private',livemode:false,plans:[{localPlanId:'11111111-1111-4111-8111-111111111111',creationOperationId:'22222222-2222-4222-8222-222222222222'}],successUrl:'https://private.example/success',cancelUrl:'https://private.example/cancel'};
  const aggregate={deploymentLabel:'owned',accountId:'acct_private',livemode:false,portal:{configurationId:'bpc_private',returnUrl:'https://private.example/return'}};
  const subscriptionRead={plans:configured.plans,collectionPolicy:'automatic_default_incomplete_v1'};
  const readAggregate={deploymentLabel:'owned',accountId:'acct_private',livemode:false,subscriptionRead};
  const paymentLinkRead={trustedPrices:["price_owned"]};
  const linkReadAggregate={deploymentLabel:"owned",accountId:"acct_private",livemode:false,paymentLinkRead};
  const expirationOnly={...aggregate,portal:undefined,portalLocalExpiration:aggregate.portal};
  const cases=[
   {name:'portal-local-expiration-only',profile:'bound-billing-v1',aggregate:expirationOnly,fail:!stripe,count:2},
   {name:'portal-local-expiration-mixed',profile:'bound-billing-v1',aggregate:{...aggregate,portalLocalExpiration:aggregate.portal},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:5},
   {name:'portal-local-expiration-stray-publisher',profile:'bound-billing-v1',aggregate:expirationOnly,publisher:'postgres://publisher@localhost/db',fail:true},
   ...[null,{}, {...aggregate.portal,create:true},{...aggregate.portal,returnUrl:null}].map((portalLocalExpiration,i)=>({name:'portal-local-expiration-invalid-'+i,profile:'bound-billing-v1',aggregate:{...expirationOnly,portalLocalExpiration},fail:true})),
   {name:'subscription-recovery-only',profile:'bound-billing-v1',aggregate:{...readAggregate,subscriptionRead:undefined,subscriptionRecovery:{...subscriptionRead,authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:2},
   {name:'subscription-create-recovery',profile:'bound-billing-v1',aggregate:{...readAggregate,subscriptionRead:undefined,subscription:{...subscriptionRead,authority:'internal_service_tenant_v1'},subscriptionRecovery:{...subscriptionRead,authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:3},
   {name:'subscription-recovery-missing-publisher',profile:'bound-billing-v1',aggregate:{...readAggregate,subscriptionRecovery:{...subscriptionRead,authority:'internal_service_tenant_v1'}},fail:true},
   ...[null,{}, {...subscriptionRead},{...subscriptionRead,authority:'end_user'},{...subscriptionRead,authority:'internal_service_tenant_v1',retry:true}].map((subscriptionRecovery,i)=>({name:'subscription-recovery-invalid-'+i,profile:'bound-billing-v1',aggregate:{...readAggregate,subscriptionRecovery},publisher:'postgres://publisher@localhost/db',fail:true})),
   {name:'subscription-create-only',profile:'bound-billing-v1',aggregate:{...readAggregate,subscription:{...subscriptionRead,authority:'internal_service_tenant_v1'},subscriptionRead:undefined},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:2},
   {name:'subscription-create-read',profile:'bound-billing-v1',aggregate:{...readAggregate,subscription:{...subscriptionRead,authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:3},
   {name:'subscription-create-missing-publisher',profile:'bound-billing-v1',aggregate:{...readAggregate,subscription:{...subscriptionRead,authority:'internal_service_tenant_v1'}},fail:true},
   ...[null,{}, {...subscriptionRead},{...subscriptionRead,authority:'end_user'},{...subscriptionRead,authority:'internal_service_tenant_v1',extra:true}].map((subscription,i)=>({name:'subscription-create-invalid-'+i,profile:'bound-billing-v1',aggregate:{...readAggregate,subscription},publisher:'postgres://publisher@localhost/db',fail:true})),
   {name:'plan-create-only',profile:'bound-billing-v1',aggregate:{deploymentLabel:"owned",accountId:"acct_private",livemode:false,plan:{trustedProducts:["custom-product"]}},fail:!stripe,count:2},
   {name:'plan-create-read',profile:'bound-billing-v1',aggregate:{deploymentLabel:"owned",accountId:"acct_private",livemode:false,plan:{trustedProducts:["custom-product"]},planRead:{trustedProducts:["custom-product"]}},fail:!stripe,count:3},
   {name:'plan-create-stray-publisher',profile:'bound-billing-v1',aggregate:{deploymentLabel:"owned",accountId:"acct_private",livemode:false,plan:{trustedProducts:["custom-product"]}},publisher:'postgres://publisher@localhost/db',fail:true},
   ...[null,{}, {trustedProducts:[]},{trustedProducts:['custom-product'],create:true},{trustedProducts:['custom-product'],publisher:'no'}].map((plan,i)=>({name:'plan-create-invalid-'+i,profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,plan},fail:true})),
   {name:'payment-link-create-only',profile:'bound-billing-v1',aggregate:{deploymentLabel:"owned",accountId:"acct_private",livemode:false,paymentLink:{trustedPrices:["price_owned"]}},fail:!stripe,count:2},
   {name:'payment-link-create-read',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLink:paymentLinkRead},fail:!stripe,count:4},
   {name:'payment-link-create-stray-publisher',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLink:paymentLinkRead},publisher:'postgres://publisher@localhost/db',fail:true},
   ...[null,{}, {trustedPrices:[]},{trustedPrices:['price_owned'],maxRecords:100},{trustedPrices:['price_owned'],publisher:'no'}].map((paymentLink,i)=>({name:'payment-link-create-invalid-'+i,profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,paymentLink},fail:true})),
   {name:'payment-link-read-limit',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:{...paymentLinkRead,maxRecords:150}},fail:!stripe,count:2},
   ...[0,1001,1.5,null].map(maxRecords=>({name:'payment-link-read-invalid-limit-'+maxRecords,profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:{...paymentLinkRead,maxRecords}},fail:true})),
   {name:'payment-link-read-only',profile:'bound-billing-v1',aggregate:linkReadAggregate,fail:!stripe,count:2},
   {name:'all-read-only',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,subscriptionRead,planRead:{trustedProducts:['custom-product']}},fail:!stripe,count:4},
   {name:'payment-link-read-stray-publisher',profile:'bound-billing-v1',aggregate:linkReadAggregate,publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'payment-link-read-mixed',profile:'bound-billing-v1',aggregate:{...aggregate,paymentLinkRead},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:5},
   {name:'payment-link-read-missing-publisher',profile:'bound-billing-v1',aggregate:{...aggregate,paymentLinkRead},fail:true},
   {name:'payment-link-read-null',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:null},fail:true},
   {name:'payment-link-read-unknown',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:{...paymentLinkRead,create:true}},fail:true},
   {name:'payment-link-read-empty',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:{trustedPrices:[]}},fail:true},

   {name:'subscription-read-only',profile:'bound-billing-v1',aggregate:readAggregate,fail:!stripe,count:1},
   {name:'subscription-plan-read',profile:'bound-billing-v1',aggregate:{...readAggregate,planRead:{trustedProducts:['custom-product']}},fail:!stripe,count:2},
   {name:'subscription-read-stray-publisher',profile:'bound-billing-v1',aggregate:readAggregate,publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'subscription-read-mixed',profile:'bound-billing-v1',aggregate:{...aggregate,subscriptionRead},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:4},
   {name:'subscription-read-missing-publisher',profile:'bound-billing-v1',aggregate:{...aggregate,subscriptionRead},fail:true},
   {name:'subscription-read-null',profile:'bound-billing-v1',aggregate:{...readAggregate,subscriptionRead:null},fail:true},
   {name:'subscription-read-unknown',profile:'bound-billing-v1',aggregate:{...readAggregate,subscriptionRead:{...subscriptionRead,create:true}},fail:true},
   {name:'subscription-read-policy',profile:'bound-billing-v1',aggregate:{...readAggregate,subscriptionRead:{...subscriptionRead,collectionPolicy:'send_invoice'}},fail:true},
   {name:'payment-link-expiration-recovery-only',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:undefined,paymentLinkExpirationRecovery:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:2},
   {name:'payment-link-expiration-recovery-missing-publisher',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:undefined,paymentLinkExpirationRecovery:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'}},fail:true},
   {name:'payment-link-expiration-recovery-mixed',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:undefined,paymentLinkExpiration:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'},paymentLinkExpirationRecovery:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:3},
   {name:'payment-link-expiration-recovery-policy-conflict',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:undefined,paymentLinkExpiration:{trustedPrices:['price_other'],authority:'internal_service_tenant_v1'},paymentLinkExpirationRecovery:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:true},
   ...[null,{trustedPrices:['price_owned']}].map((paymentLinkExpirationRecovery,i)=>({name:'payment-link-expiration-recovery-invalid-'+i,profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkRead:undefined,paymentLinkExpirationRecovery},publisher:'postgres://publisher@localhost/db',fail:true})),
   {name:'payment-link-expiration-only',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,paymentLinkExpiration:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:2},
   {name:'payment-link-expiration-missing-publisher',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,paymentLinkExpiration:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'}},fail:true},
   {name:'payment-link-expiration-missing-authority',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,paymentLinkExpiration:{trustedPrices:['price_owned']}},publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'payment-link-expiration-read-mixed',profile:'bound-billing-v1',aggregate:{...linkReadAggregate,paymentLinkExpiration:{trustedPrices:['price_owned'],authority:'internal_service_tenant_v1'}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:4},
   {name:'plan-read-only',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,planRead:{trustedProducts:['custom-product']}},fail:!stripe,count:1},
   {name:'plan-read-stray-publisher',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,planRead:{trustedProducts:['custom-product']}},publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'plan-read-mixed',profile:'bound-billing-v1',aggregate:{...aggregate,planRead:{trustedProducts:['custom-product']}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:4},
   {name:'plan-read-mixed-missing-publisher',profile:'bound-billing-v1',aggregate:{...aggregate,planRead:{trustedProducts:['custom-product']}},fail:true},
   {name:'plan-read-empty',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,planRead:{trustedProducts:[]}},fail:true},
   {name:'plan-read-null',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,planRead:null},fail:true},
   {name:'plan-read-unknown',profile:'bound-billing-v1',aggregate:{deploymentLabel:'owned',accountId:'acct_private',livemode:false,planRead:{trustedProducts:['custom-product'],create:true}},fail:true},
   {name:'aggregate-portal',profile:'bound-billing-v1',aggregate,publisher:'postgres://publisher@localhost/db',fail:!stripe,count:3},
   {name:'aggregate-both',profile:'bound-billing-v1',aggregate:{...aggregate,checkout:{plans:configured.plans,successUrl:configured.successUrl,cancelUrl:configured.cancelUrl}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:6},
   {name:'aggregate-checkout',profile:'bound-billing-v1',aggregate:{deploymentLabel:aggregate.deploymentLabel,accountId:aggregate.accountId,livemode:false,checkout:{plans:configured.plans,successUrl:configured.successUrl,cancelUrl:configured.cancelUrl}},publisher:'postgres://publisher@localhost/db',fail:!stripe,count:3},
   {name:'aggregate-mixed',profile:'bound-billing-v1',aggregate,config:configured,publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'aggregate-missing-publisher',profile:'bound-billing-v1',aggregate,fail:true},
   {name:'aggregate-missing-config',profile:'bound-billing-v1',publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'aggregate-null',profile:'bound-billing-v1',aggregate:{...aggregate,portal:null},publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'old-mixed',profile:'bound-checkout-v1',aggregate,config:configured,publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'legacy-mixed',profile:'legacy',aggregate,fail:true},
   {name:'legacy',profile:'legacy',config:null,publisher:null,fail:false},
   {name:'bound',profile:'bound-checkout-v1',config:configured,publisher:'postgres://publisher:private-password@localhost/db',fail:!stripe},
   {name:'missing-publisher',profile:'bound-checkout-v1',config:configured,publisher:null,fail:true},
   {name:'missing-config',profile:'bound-checkout-v1',config:null,publisher:'postgres://publisher@localhost/db',fail:true},
   {name:'stray',profile:'legacy',config:configured,publisher:null,fail:true},
   {name:'unknown',profile:'unknown',config:null,publisher:null,fail:true},
   {name:'unknown-config-field',profile:'bound-checkout-v1',config:{...configured,stripeFields:{}},publisher:'postgres://publisher@localhost/db',fail:true}
  ];
  testedCases += cases.length;
  cases.sort((a,b)=>(a.name==='legacy'?-1:b.name==='legacy'?1:0));
  let legacy;
  for(const test of cases){
   const directory=path.join(temporary,`${stripe}-${test.name}`);
   const env={ARTIFACT_DIR:directory,BILLING_REQUEST_DATE_PROFILE:'utc-iso-and-epoch',BILLING_IDENTITY_PROFILE:test.profile,EXPECT_FAILURE:test.fail?'1':'0'};
   if(test.config)env.BILLING_BOUND_CHECKOUT_CONFIG=JSON.stringify(test.config);
   if(test.aggregate)env.BILLING_BOUND_BILLING_CONFIG=JSON.stringify(test.aggregate);
   if(test.publisher)env.BILLING_PUBLISHER_DATABASE_URL=test.publisher;
   run(path.join(target,'debug/fl-billing-bound-checkout-artifacts'),[],env);
   if(test.fail)continue;
   const contract=JSON.parse(readFileSync(path.join(directory,'contract.json'))),openapi=JSON.parse(readFileSync(path.join(directory,'openapi.json')));
   const updateRoute=contract.routes.find(r=>r.operationName==='updatePaymentLink');
   const updateSchema=typeof updateRoute.request.body.schema.jsonSchema==='string'?JSON.parse(updateRoute.request.body.schema.jsonSchema):updateRoute.request.body.schema.jsonSchema;
   if(stripe){assert.equal(updateSchema.properties.amount['x-forklaunch-coercion'],'typescript-number');assert.equal(updateSchema.properties.amount.type,undefined);}else assert.equal(updateSchema.properties.amount.type,'number');
   for(const route of contract.routes) {
    const selected=stripe && ['/payment-link','/checkout-session','/subscription','/plan'].some(prefix=>route.path===prefix||route.path.startsWith(prefix+'/')) && route.auth?.method==='hmac';
    if(selected)for(const status of [401,403])assert.equal(route.responses.find(r=>r.statusCode===status)?.body?.contentType,'text/plain');
   }
   const authResponse=openapi.paths['/payment-link/{id}'].put.responses['401'];
   if(stripe)assert(authResponse.content['text/plain']);else assert.equal(authResponse,undefined);
   for(const route of contract.routes.filter(r=>r.path.startsWith('/billing-portal')||r.path.startsWith('/bound-')))for(const status of [401,403]){const response=route.responses.find(r=>r.statusCode===status);if(response)assert.notEqual(response.body?.contentType,'text/plain');}
   const listRoute=contract.routes.find(r=>r.operationName==='listBoundCreatedPaymentLinks');
   if(listRoute){const field=listRoute.request.query.find(v=>v.name==='ids');assert.equal(field.required,true);const schema=typeof field.schema.jsonSchema==='string'?JSON.parse(field.schema.jsonSchema):field.schema.jsonSchema;assert.equal(schema.type,'array');assert.equal(schema.minItems,1);assert.equal(schema.maxItems,test.name==='payment-link-read-limit'?150:100);assert.equal(schema.items.format,'uuid');}
   const text=JSON.stringify({contract,openapi});assert(text.includes('x-forklaunch-date-profile'));
   for(const secret of ['acct_private','private-password','private.example'])assert(!text.includes(secret));
   const bound=contract.routes.filter(r=>(r.path.startsWith('/bound-checkout/')||r.path.startsWith('/bound-portal/')||(r.path.startsWith('/bound-plans')||r.path.startsWith('/bound-plan-operations/'))||(r.path.startsWith('/bound-subscriptions')||r.path.startsWith('/bound-subscription-operations/'))||(r.path.startsWith('/bound-payment-links')||r.path.startsWith('/bound-payment-link-operations/'))));
   if(test.profile==='legacy'){assert.equal(bound.length,0);legacy=contract.routes;}
   else {assert.equal(bound.length,test.count??3);assert.deepEqual(contract.routes.filter(r=>!(r.path.startsWith('/bound-checkout/')||r.path.startsWith('/bound-portal/')||(r.path.startsWith('/bound-plans')||r.path.startsWith('/bound-plan-operations/'))||(r.path.startsWith('/bound-subscriptions')||r.path.startsWith('/bound-subscription-operations/'))||(r.path.startsWith('/bound-payment-links')||r.path.startsWith('/bound-payment-link-operations/')))),legacy);if(['plan-read-only','subscription-read-only','subscription-plan-read','payment-link-read-only','payment-link-read-limit','all-read-only'].includes(test.name)){assert(bound.every(r=>['getBoundCreatedPlan','getBoundCreatedSubscription','getBoundCreatedPaymentLink','listBoundCreatedPaymentLinks'].includes(r.operationName)));assert(bound.every(r=>!r.responses.some(v=>v.statusCode===202)));if(test.name==='subscription-read-only')assert.equal(bound[0].operationName,'getBoundCreatedSubscription');if(test.name==='payment-link-read-only')assert.equal(bound[0].operationName,'getBoundCreatedPaymentLink');}else assert(bound.some(r=>r.responses.some(v=>v.statusCode===202)));}
  }

 }
 // Startup wiring lives in the module's registrations; generated module builds compile it.
 const registrationsTemplate=readFileSync(path.join(root,'cli/src/templates/rust/billing/registrations.rs'),'utf8');
 const manifestTemplate=readFileSync(path.join(root,'cli/src/templates/rust/billing/Cargo.toml.template'),'utf8');
 for(const stripe of [false,true]) {
  const render=text=>text.replace(/{{#stripe}}([\s\S]*?){{\/stripe}}/g,(_,body)=>stripe?body:'').replace(/{{\^stripe}}([\s\S]*?){{\/stripe}}/g,(_,body)=>stripe?'':body);
  assert.equal(render(manifestTemplate).includes('"typescript-routing"'),stripe,'only generated Stripe opts into pinned case routing');
  const wiring=render(registrationsTemplate).replace(/\s+/g,'');
  assert.equal(wiring.includes('BillingBackend::stripe(store,client).with_configured_hmac(true).with_configured_subscription_updates(true)'),stripe,'generated Stripe backend selects configured owner preflight; base template does not');
  assert.equal(wiring.includes('.with_configured_cached_reads(true)'),stripe,'generated Stripe backend selects configured cached GET mapping; generic base stays unchanged');
  assert.equal(wiring.includes('.with_configured_payment_link_updates(true)'),stripe,'generated Stripe backend selects configured PaymentLink update preflight independently from reads');
 }
 console.log(`PASS actual rendered generated library: ${testedCases} base/Stripe identity startup/export cases, shared selected-family registry, unchanged default routes, Date composition, no private config in metadata; base/Stripe startup wiring selections`);
} finally {rmSync(temporary,{recursive:true,force:true});}
