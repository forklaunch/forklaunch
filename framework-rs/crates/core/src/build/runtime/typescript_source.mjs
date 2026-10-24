// Framework-owned build adapter. Syntax admission uses OXC from the canonical build
// tool; importing the application server or dependency container is forbidden.
import { realpath, readFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const service = await realpath(process.argv[1]);
const { generateOpenApiSpecs } = await import('@forklaunch/core/http');
const validatorName = process.argv[2];
if (!['zod', 'typebox'].includes(validatorName)) throw new Error('Unsupported contract validator');
const { SchemaValidator } = await import(`@forklaunch/validator/${validatorName}`);
const validator = SchemaValidator();
process.stdin.setEncoding('utf8');
let input = '';
for await (const chunk of process.stdin) input += chunk;
const { files } = JSON.parse(input);
function ast(file) {
  const result = files[file];
  if (!result) throw new Error('Contract source was not admitted by the build tool: ' + file);
  return result;
}
function visit(node, callback) {
  if (!node || typeof node !== 'object') return;
  callback(node);
  for (const value of Object.values(node)) {
    if (Array.isArray(value)) value.forEach(child => visit(child, callback));
    else if (value && typeof value === 'object') visit(value, callback);
  }
}
function member(node) {
  if (node?.type === 'Identifier') return node.name;
  if (node?.type === 'MemberExpression' && !node.computed) return `${member(node.object)}.${member(node.property)}`;
  return undefined;
}
const source = ast(path.join(service, 'server.ts'));
const imports = new Map();
for (const statement of source.body) {
  if (statement.type !== 'ImportDeclaration') continue;
  for (const specifier of statement.specifiers) imports.set(specifier.local.name, statement.source.value);
}
const servicePackage = JSON.parse(await readFile(path.join(service, 'package.json'), 'utf8'));
const corePackage = servicePackage.name.replace(/\/[^/]+$/, '/core');
const delegatedHelpers = new Set();
for (const statement of source.body) if (statement.type === 'ImportDeclaration' && statement.source.value === corePackage) {
  for (const specifier of statement.specifiers) if (specifier.type === 'ImportSpecifier' && specifier.imported.name === 'registerDelegatedSurface') delegatedHelpers.add(specifier.local.name);
}
const delegatedCalls = [], delegatedRoutes = new Set();
visit(source, node => {
  if (node.type !== 'CallExpression' || !delegatedHelpers.has(member(node.callee))) return;
  const [descriptor, mount, callback] = node.arguments;
  if (node.arguments.length !== 3 || descriptor?.type !== 'Identifier' || mount?.type !== 'ObjectExpression' ||
      mount.properties.length !== 2 || mount.properties.some(property => property.type !== 'Property' || property.computed || !['basePath', 'paths'].includes(member(property.key))) ||
      callback?.type !== 'ArrowFunctionExpression' || callback.params.length !== 1 || callback.params[0].type !== 'Identifier') throw new Error('Delegated surface requires an explicit descriptor, runtime mount and scoped registration');
  const binding = callback.params[0].name;
  const callbacks = [];
  let catchAll = 0;
  // A provider mount may be registered once per pattern
  // (`for (const path of ...mount.paths...) app.internal.all(path, ...)`),
  // because some adapters accept one pattern per registration.
  const mountPathBindings = new Set();
  visit(callback.body, loop => {
    if (loop.type !== 'ForOfStatement' || loop.left?.type !== 'VariableDeclaration' || loop.left.declarations.length !== 1 ||
        loop.left.declarations[0].id?.type !== 'Identifier') return;
    let readsPaths = false;
    visit(loop.right, node => { if (member(node) === `${binding}.paths`) readsPaths = true; });
    if (readsPaths) mountPathBindings.add(loop.left.declarations[0].id.name);
  });
  visit(callback.body, route => {
    if (route.type !== 'CallExpression' || !/^app\.(?:internal\.)?(all|get|post|put|patch|delete|head|options|trace)$/.test(member(route.callee) || '')) return;
    const method = member(route.callee), routePath = route.arguments[0];
    if (method === 'app.internal.all' && (member(routePath) === `${binding}.paths` ||
        (routePath?.type === 'Identifier' && mountPathBindings.has(routePath.name)))) catchAll++;
    else if (method === 'app.internal.get' && routePath?.type === 'TemplateLiteral' && routePath.expressions.length === 1 &&
        member(routePath.expressions[0]) === `${binding}.basePath` && routePath.quasis.length === 2 && routePath.quasis[0].value.cooked === '') callbacks.push(routePath.quasis[1].value.cooked);
    else throw new Error('Undeclared route in delegated surface registration');
    delegatedRoutes.add(route);
  });
  if (catchAll !== 1 || new Set(callbacks).size !== callbacks.length) throw new Error('Delegated surface requires one provider mount and unique callbacks');
  delegatedCalls.push({descriptor: descriptor.name, callbacks});
});
const mounted = [];
visit(source, node => {
  if (node.type !== 'CallExpression') return;
  if (!delegatedRoutes.has(node) && /^app\.(?:internal\.)?(all|get|post|put|patch|delete|head|options|trace)$/.test(member(node.callee) || '')) throw new Error('Direct application routes need an explicit shared router contract');
  if (member(node.callee) !== 'app.use') return;
  if (node.arguments.length !== 1 || node.arguments[0].type !== 'Identifier') throw new Error('Contract export requires explicit app.use(router) mounts');
  const imported = imports.get(node.arguments[0].name);
  if (!imported?.startsWith('./api/routes/') || !imported.endsWith('.routes')) throw new Error('Mounted router requires an explicit local contract module');
  mounted.push(imported.slice('./api/routes/'.length, -'.routes'.length));
});
if (!mounted.length || new Set(mounted).size !== mounted.length) throw new Error('Missing or duplicate contract router mounts');
const seen = new Set();
async function checkImports(file) {
  file = await realpath(file);
  if (!file.startsWith(service + path.sep)) throw new Error('Contract imports must remain inside the service');
  if (seen.has(file)) return;
  seen.add(file);
  const syntax = ast(file);
  visit(syntax, node => {
    if (node.type === 'ImportExpression' || node.type === 'CallExpression' && member(node.callee) === 'require') throw new Error('Dynamic imports are not admitted in isolated contracts');
  });
  for (const statement of syntax.body) {
    if (!['ImportDeclaration', 'ExportNamedDeclaration', 'ExportAllDeclaration'].includes(statement.type) || !statement.source) continue;
    if (statement.importKind === 'type' || statement.exportKind === 'type') continue;
    if (statement.type === 'ImportDeclaration' && statement.specifiers.length && statement.specifiers.every(specifier => specifier.importKind === 'type')) continue;
    const specifier = statement.source.value;
    if (specifier === `@forklaunch/validator/${validatorName}`) continue;
    if (specifier === '@forklaunch/implementation-iam-base/schemas') continue;
    if (specifier === '@forklaunch/core/mappers' && statement.type === 'ImportDeclaration' && statement.specifiers.length === 1 && statement.specifiers[0].type === 'ImportSpecifier' && statement.specifiers[0].imported.name === 'mapServiceSchemas') continue;
    if (!specifier.startsWith('.')) throw new Error('Contract import requires a pure local schema or validator module: ' + specifier);
    const stem = path.resolve(path.dirname(file), specifier.replace(/\.js$/, ''));
    const target = files[stem + '.ts'] ? stem + '.ts' : path.join(stem, 'index.ts');
    if (!target.includes(`${path.sep}domain${path.sep}enum${path.sep}`) && !target.includes(`${path.sep}domain${path.sep}schemas${path.sep}`) && !target.includes(`${path.sep}api${path.sep}contracts${path.sep}`)) throw new Error('Contract export cannot import runtime infrastructure: ' + specifier);
    await checkImports(target);
  }
}
const routers = [];
const portablePolicies = new Map();
const delegatedSurfaces = [];
for (const call of delegatedCalls) {
  const specifier = imports.get(call.descriptor);
  if (!specifier?.startsWith('./api/contracts/') || !specifier.endsWith('.surface')) throw new Error('Delegated descriptor must be a pure local surface module');
  const file = path.resolve(service, specifier + '.ts');
  await checkImports(file);
  const statement = source.body.find(statement => statement.type === 'ImportDeclaration' && statement.source.value === specifier);
  const binding = statement.specifiers.find(binding => binding.local.name === call.descriptor);
  if (binding.type !== 'ImportSpecifier') throw new Error('Delegated descriptor requires a named import');
  const descriptor = (await import(pathToFileURL(file).href))[binding.imported.name];
  if (!descriptor || typeof descriptor.id !== 'string' || !descriptor.id || typeof descriptor.provider !== 'string' || !descriptor.provider || descriptor.coverage !== 'provider-owned' ||
      typeof descriptor.client?.package !== 'string' || !descriptor.client.package || typeof descriptor.client?.export !== 'string' || !descriptor.client.export ||
      descriptor.runtime?.basePath !== 'runtime-resolved' || typeof descriptor.runtime?.openApi !== 'boolean' ||
      !Array.isArray(descriptor.callbacks) || descriptor.callbacks.some(value => typeof value !== 'string' || !value.startsWith('/')) ||
      JSON.stringify([...descriptor.callbacks].sort()) !== JSON.stringify([...call.callbacks].sort()) ||
      delegatedSurfaces.some(surface => surface.id === descriptor.id)) throw new Error('Invalid or mismatched delegated surface descriptor');
  if (Object.keys(descriptor).some(key => !['id','provider','coverage','client','runtime','callbacks'].includes(key)) ||
      Object.keys(descriptor.client).some(key => !['package','export'].includes(key)) ||
      Object.keys(descriptor.runtime).some(key => !['basePath','openApi'].includes(key))) throw new Error('Delegated descriptor cannot include runtime credentials or undeclared metadata');
  delegatedSurfaces.push(descriptor);
}
for (const name of mounted) {
  const contractFile = path.join(service, 'api/contracts', `${name}.contract.ts`);
  await checkImports(contractFile);
  const runtime = ast(path.join(service, 'api/routes', `${name}.routes.ts`));
  let binding;
  for (const statement of runtime.body) {
    if (statement.type !== 'ImportDeclaration' || statement.source.value !== `../contracts/${name}.contract`) continue;
    if (statement.specifiers.length === 1) binding = statement.specifiers[0].local.name;
  }
  if (!binding) throw new Error('Runtime router must import its shared contract');
  let sharedBase = false;
  const registered = new Map();
  const routeHandlers = new Map();
  for (const statement of runtime.body) {
    if (statement.type === 'ImportDeclaration' && statement.source.value === `../controllers/${name}.controller`) {
      for (const specifier of statement.specifiers) if (specifier.type === 'ImportSpecifier') routeHandlers.set(specifier.local.name, specifier.imported.name);
    }
  }
  visit(runtime, node => {
    if (node.type !== 'CallExpression') return;
    if (member(node.callee) === 'forklaunchRouter') sharedBase = member(node.arguments[0]) === `${binding}.basePath`;
    if (node.callee.type === 'MemberExpression' && member(node.callee.object)?.endsWith('Router')) {
      const method = member(node.callee.property);
      if (['get', 'post', 'put', 'patch', 'delete', 'head', 'options', 'trace'].includes(method)) {
        const contractPath = member(node.arguments[0]);
        if (!contractPath?.startsWith(`${binding}.`) || !contractPath.endsWith('.path')) throw new Error('Runtime route path must consume its shared contract');
        const key = contractPath.slice(binding.length + 1, -'.path'.length);
        if (!key || key.includes('.') || registered.has(key)) throw new Error('Duplicate or invalid contract operation binding');
        const handler = routeHandlers.get(member(node.arguments[1]));
        if (!handler) throw new Error('Runtime route requires its explicit controller handler');
        registered.set(key, {method, handler});
      }
    }
  });
  if (!sharedBase) throw new Error('Runtime router base path must consume its shared contract');
  const controller = ast(path.join(service, 'api/controllers', `${name}.controller.ts`));
  let controllerBinding;
  const authBinders = new Set();
  for (const statement of controller.body) {
    if (statement.type === 'ImportDeclaration' && statement.source.value === corePackage) {
      for (const specifier of statement.specifiers) if (specifier.type === 'ImportSpecifier' && specifier.imported.name === 'bindRuntimeAuth') authBinders.add(specifier.local.name);
    }
    if (statement.type === 'ImportDeclaration' && statement.source.value === `../contracts/${name}.contract` && statement.specifiers.length === 1) controllerBinding = statement.specifiers[0].local.name;
  }
  if (!controllerBinding) throw new Error('Runtime controller must import its shared contract');
  const definitions = new Map();
  for (const statement of controller.body) {
    const declaration = statement.type === 'ExportNamedDeclaration' ? statement.declaration : undefined;
    if (declaration?.type === 'VariableDeclaration') for (const variable of declaration.declarations) {
      if (variable.id.type === 'Identifier' && variable.init?.type === 'CallExpression') definitions.set(variable.init, variable.id.name);
    }
  }
  const handled = new Set();
  const authBound = new Map();
  visit(controller, node => {
    if (node.type !== 'CallExpression' || node.callee.type !== 'MemberExpression' || member(node.callee.object) !== 'handlers') return;
    const method = member(node.callee.property);
    const contractPath = member(node.arguments[1]);
    if (!contractPath?.startsWith(`${controllerBinding}.`) || !contractPath.endsWith('.path')) throw new Error('Runtime handler path must consume its shared contract');
    const key = contractPath.slice(controllerBinding.length + 1, -'.path'.length);
    const registeredOperation = registered.get(key);
    if (!registeredOperation || registeredOperation.method !== method || registeredOperation.handler !== definitions.get(node)) throw new Error('Controller handler has no matching contract operation');
    const details = node.arguments[2];
    const shared = `${controllerBinding}.${key}.details`;
    const bound = details?.type === 'CallExpression' && authBinders.has(member(details.callee)) && details.arguments.length === 2 && member(details.arguments[0]) === shared;
    if (bound) {
      const keys = details.arguments[1];
      if (keys?.type !== 'ObjectExpression' || keys.properties.some(property => property.type !== 'Property' || property.computed || !['jwksPublicKeyUrl', 'signatureKey', 'secretKeys', 'decodeResource'].includes(member(property.key)))) throw new Error('Runtime authentication binding may supply credentials and resource decoding only');
      const fields = keys.properties.map(property => member(property.key));
      if (new Set(fields).size !== fields.length) throw new Error('Duplicate authentication binding field');
      authBound.set(key, fields);
    } else if (member(details) !== shared && !(details?.type === 'ObjectExpression' && details.properties.length === 1 && details.properties[0].type === 'SpreadElement' && member(details.properties[0].argument) === shared)) throw new Error('Runtime handler metadata must consume its shared contract without overrides');
    if (handled.has(key)) throw new Error('Duplicate controller operation');
    handled.add(key);
  });
  if (handled.size !== registered.size) throw new Error('Contract operation is missing its shared controller');

  const exported = await import(pathToFileURL(contractFile).href);
  const candidates = Object.values(exported).filter(value => value && typeof value === 'object' && typeof value.basePath === 'string');
  if (candidates.length !== 1) throw new Error('Expected one explicit router contract per module');
  const contract = candidates[0];
  const routes = [], sdkPaths = {};
  for (const [key, operation] of Object.entries(contract)) {
    if (key === 'basePath') continue;
    const registeredOperation = registered.get(key);
    const method = operation?.method;
    if (!registeredOperation || registeredOperation.method !== method) throw new Error('Contract operation has no matching runtime route');
    registered.delete(key);
    if (!operation || typeof operation.path !== 'string' || typeof operation.operationId !== 'string' || !operation.details) throw new Error('Invalid explicit route contract');
    if (operation.details.access === 'protected' && !operation.details.auth) throw new Error('Protected contract must declare its authentication scheme before SDK export');
    if (operation.details.auth) {
      const policy = operation.details.auth;
      const bindingFields = authBound.get(key);
      const jwt = policy.scheme === 'jwt';
      const allowed = jwt ? ['scheme', 'headerName', 'tokenPrefix', 'sessionSchema', 'allowedRoles', 'allowedPermissions'] : ['scheme', 'headerName', 'tokenPrefix'];
      if (!bindingFields || !['jwt', 'hmac'].includes(policy.scheme) || Object.keys(policy).some(key => !allowed.includes(key))) throw new Error('Portable authentication policy must use the core runtime authentication binder');
      const credentials = bindingFields.filter(field => field !== 'decodeResource');
      if (credentials.length !== 1 || ! (jwt ? ['jwksPublicKeyUrl', 'signatureKey'] : ['secretKeys']).includes(credentials[0]) || !jwt && bindingFields.length !== 1) throw new Error('Runtime authentication credentials do not match the portable policy');
      if (jwt && policy.allowedRoles === undefined && policy.allowedPermissions === undefined) throw new Error('Portable JWT policy requires explicit authorization');
      for (const field of ['allowedRoles', 'allowedPermissions']) if (policy[field] !== undefined && (!Array.isArray(policy[field]) || policy[field].some(value => typeof value !== 'string'))) throw new Error('Invalid portable authorization policy');
      for (const field of ['headerName', 'tokenPrefix']) if (policy[field] !== undefined && (typeof policy[field] !== 'string' || !policy[field])) throw new Error('Invalid portable authentication header');
      const portable = {...policy};
      if (portable.sessionSchema) portable.sessionSchema = validator.openapi(portable.sessionSchema);
      portablePolicies.set(operation.operationId, portable);
    } else if (authBound.has(key)) throw new Error('Runtime authentication binding requires portable policy');
    routes.push({ method, path: operation.path, contractDetails: operation.details });
    sdkPaths[`${method}.${operation.path}`] = operation.operationId;
  }
  if (registered.size) throw new Error('Runtime operation is missing from its contract');
  routers.push({ basePath: contract.basePath, routers: [], routes, sdkPaths });
}
const documents = generateOpenApiSpecs(validator, [], [], { basePath: '/', routers, routes: [], sdkPaths: {} });
for (const version of Reflect.ownKeys(documents)) {
  if (delegatedSurfaces.length) documents[version]['x-forklaunch-delegated-surfaces'] = delegatedSurfaces;
  for (const item of Object.values(documents[version].paths || {})) for (const operation of Object.values(item)) {
    const policy = portablePolicies.get(operation?.operationId);
    if (policy) {
      operation['x-forklaunch-auth'] = policy;
      if (policy.scheme === 'hmac') {
        const header = policy.headerName || 'Authorization';
        const name = `ForkLaunchHmac-${header}`;
        documents[version].components ||= {};
        documents[version].components.securitySchemes ||= {};
        documents[version].components.securitySchemes[name] = {type: 'apiKey', in: 'header', name: header, description: 'ForkLaunch HMAC authentication'};
        operation.security = [{[name]: []}];
      }
    }
  }
}
process.stdout.write(JSON.stringify(Object.fromEntries(Reflect.ownKeys(documents).map(version => [typeof version === 'symbol' ? '' : version, documents[version]]))));
