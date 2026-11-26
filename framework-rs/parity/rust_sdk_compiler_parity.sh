#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
if [ "${FORKLAUNCH_WORKER_RUNTIME_LOCKED:-}" != 1 ]; then
    exec python3 "$SCRIPT_DIR/fixtures/worker_runtime_lock.py" "${BASH_SOURCE[0]}" "$@"
fi
CLI="${FORKLAUNCH_CLI:-$REPO_ROOT/cli/target/debug/forklaunch}"
TSC="${FORKLAUNCH_TEST_TSC:-$REPO_ROOT/framework/node_modules/.bin/tsc}"
RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/sdk-compiler-parity.XXXXXX")"
echo "SDK parity workspace: $RUN_ROOT"
"$CLI" init application sdk-review --path "$RUN_ROOT/app" --language rust -D SDK -A ForkLaunch -L MIT
"$CLI" init service api --path "$RUN_ROOT/app" --language rust -D API
cd "$RUN_ROOT/app"
cargo check -p api
cp src/client-sdk/typescript/api/index.ts "$RUN_ROOT/build.ts"
cp src/client-sdk/rust/api.rs "$RUN_ROOT/build.rs"
test -s src/client-sdk/typescript/api/openapi.json
"$CLI" openapi export --path .
"$CLI" sdk generate --path . --target all --from contract
cmp "$RUN_ROOT/build.ts" src/client-sdk/typescript/api/index.ts
cmp "$RUN_ROOT/build.rs" src/client-sdk/rust/api-sdk/lib.rs
# Move the service below an additional directory while retaining the registered path.
python3 - <<'PYTHON'
from pathlib import Path
source=Path('src/api'); target=Path('src/team/api'); target.parent.mkdir()
source.rename(target)
for manifest in target.rglob('Cargo.toml'):
    text=manifest.read_text().replace('../../vendor/', '../../../vendor/').replace('../../../.forklaunch/', '../../../../.forklaunch/').replace('../../.forklaunch/', '../../../.forklaunch/')
    manifest.write_text(text)
for path in ['Cargo.toml','.forklaunch/manifest.toml']:
    p=Path(path);p.write_text(p.read_text().replace('src/api','src/team/api'))
Path('src/client-sdk/rust/api.rs').unlink()
PYTHON
cargo check -p api
test -s src/client-sdk/rust/api.rs
test ! -e src/team/client-sdk
"$CLI" sdk generate --path . --target all --from contract --check
printf '\n// intentional stale marker\n' >> src/client-sdk/typescript/api/index.ts
if "$CLI" sdk generate --path . --target all --from contract --check; then echo 'Expected stale SDK rejection'; exit 1; fi
grep -q 'intentional stale marker' src/client-sdk/typescript/api/index.ts
cp "$RUN_ROOT/build.ts" src/client-sdk/typescript/api/index.ts
node --input-type=module <<'JS'
import fs from 'node:fs';
const path='.forklaunch/contracts/api/contract.json', artifact=JSON.parse(fs.readFileSync(path,'utf8'));
const field=(name,schema)=>({name,required:true,schema:{name:'Value',jsonSchema:schema}});
artifact.routes=[{name:'GetItem',operationName:'getItem',summary:'Read item',method:'GET',path:'/items/:id',request:{
 params:[field('id',{type:'string',format:'uuid'})],query:[field('limit',{type:'integer'})],headers:[field('x-token',{type:'string'})]
},responses:[{statusCode:200,headers:[],body:{kind:'json',contentType:'application/json',schema:{name:'Item',jsonSchema:{type:'object',properties:{count:{type:'integer'}},required:['count'],additionalProperties:false}}}},
{statusCode:200,headers:[],body:{kind:'text',contentType:'text/plain',schema:{name:'Text',jsonSchema:{type:'string'}}}}]}];
fs.writeFileSync(path,JSON.stringify(artifact));
JS
"$CLI" sdk generate --path . --target all --from contract
"$CLI" sdk generate --path . --target all --from contract --check
cat > consumer.ts <<'TS'
import {createForkLaunchSdk} from './src/client-sdk/typescript/index.js';
const sdk=createForkLaunchSdk();
const request={params:{id:'33333333-3333-4333-8333-333333333333'},query:{limit:2},headers:{'x-token':'test'}};
const response=await sdk.api.items.getItem(request);
// @ts-expect-error required contract header cannot be omitted
sdk.api.items.getItem({params:request.params,query:request.query});
// @ts-expect-error numeric query field cannot be a string
sdk.api.items.getItem({...request,query:{limit:'two'}});
if(response.status===200){
 if(response.contentType==='application/json') response.response.count.toFixed();
 if(response.contentType==='text/plain') response.response.toUpperCase();
}
TS
"$TSC" --noEmit --strict --skipLibCheck --target es2022 --module esnext --moduleResolution bundler consumer.ts
cargo check -p api-sdk
# Artifact-only edits must not be overwritten by rebuilding application contracts.
node -e 'const a=require("./.forklaunch/contracts/api/contract.json");if(a.routes[0].operationName!=="getItem")throw Error("ignored supplied contract")'
"$CLI" change service --path src/team/api -N renamed-api --confirm
test ! -e src/client-sdk/typescript/api
test ! -e src/client-sdk/rust/api.rs
cargo check -p renamed-api
test -s src/client-sdk/rust/renamed-api.rs
"$CLI" delete service renamed-api --path . --continue
test ! -e src/client-sdk/typescript/renamed-api
test ! -e src/client-sdk/rust/renamed-api.rs
echo 'PASS build/CLI SDK byte parity, artifact-driven generation, stale check preservation, typed metadata, same-status media and relocated rename/delete lifecycle'
rm -rf "$RUN_ROOT"
