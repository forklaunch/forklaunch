// Source-level public API inventory. No application or package code is executed.
import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import {existsSync, readFileSync, realpathSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const compiler = process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc');
const ts = require(require.resolve('typescript', {paths: [path.dirname(realpathSync(compiler))]}));
const relative = file => path.relative(root, file).split(path.sep).join('/');
const inventory = readFileSync(path.join(root, 'plan/RUST-TYPESCRIPT-PARITY-INVENTORY.md'), 'utf8');
const manifests = [...new Set([...inventory.split('## CLI extension surfaces')[0]
  .matchAll(/\]\(\.\.\/([^)]*\/package\.json)\)/g)].map(match => match[1]))].sort();
const trackedManifests = execFileSync('git', ['ls-files', 'framework/**/package.json', 'blueprint/**/package.json'], {cwd: root, encoding: 'utf8'}).trim().split('\n').sort();
if (JSON.stringify(manifests) !== JSON.stringify(trackedManifests)) throw new Error('Package table does not cover every tracked framework/blueprint package');
const asts = new Map();
function ast(file) {
  if (!asts.has(file)) asts.set(file, ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true));
  return asts.get(file);
}
function resolveLocal(file, specifier) {
  if (!specifier.startsWith('.')) return undefined;
  const base = path.resolve(path.dirname(file), specifier).replace(/\.(?:mjs|cjs|js)$/, '');
  return [base + '.ts', base + '.tsx', base + '.mts', path.join(base, 'index.ts'), base + '.d.ts']
    .find(candidate => existsSync(candidate));
}
function location(node, file) {
  return {source: relative(file), line: ast(file).getLineAndCharacterOfPosition(node.getStart()).line + 1};
}
function names(node) {
  if (ts.isIdentifier(node)) return [node.text];
  if (ts.isObjectBindingPattern(node) || ts.isArrayBindingPattern(node))
    return node.elements.flatMap(element => ts.isBindingElement(element) ? names(element.name) : []);
  return [node.getText()];
}
function declaration(node, file, name) {
  const item = {name, kind: ts.SyntaxKind[node.kind], ...location(node, file)};
  // Preserve public DTO/service members and signatures, including overloads.
  if (node.members) item.members = node.members.filter(member => !member.modifiers?.some(modifier =>
    modifier.kind === ts.SyntaxKind.PrivateKeyword || modifier.kind === ts.SyntaxKind.ProtectedKeyword))
    .filter(member => !member.name || !ts.isPrivateIdentifier(member.name))
    .map(member => ({name: member.name?.getText() ?? (ts.isConstructorDeclaration(member) ? 'constructor' : '(signature)'),
      signature: member.getText().slice(0, member.body ? member.body.getStart() - member.getStart() : undefined).trim(),
      ...location(member, file)}));
  if (ts.isTypeAliasDeclaration(node)) item.type = node.type.getText();
  return item;
}
function exportsOf(file) {
  const result = [];
  for (const node of ast(file).statements) {
    if (ts.isExportDeclaration(node)) {
      const specifier = node.moduleSpecifier?.text;
      const target = specifier && resolveLocal(file, specifier);
      result.push({kind: 'reexport', ...location(node, file),
        ...(specifier ? {specifier, ...(target ? {target: relative(target)} : {external: !specifier.startsWith('.'), unresolved: specifier.startsWith('.')})} : {}),
        exports: !node.exportClause ? '*' : ts.isNamedExports(node.exportClause)
          ? node.exportClause.elements.map(element => ({name: element.name.text, original: element.propertyName?.text ?? element.name.text, typeOnly: node.isTypeOnly || element.isTypeOnly}))
          : [{name: node.exportClause.name.text, original: '*', typeOnly: node.isTypeOnly}]});
    } else if (ts.isExportAssignment(node)) {
      result.push({kind: 'assignment', name: node.isExportEquals ? 'export=' : 'default', expression: node.expression.getText(), ...location(node, file)});
    } else if (node.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword)) {
      if (ts.isVariableStatement(node)) {
        for (const variable of node.declarationList.declarations)
          for (const name of names(variable.name)) result.push(declaration(variable, file, name));
      } else result.push(declaration(node, file, node.modifiers.some(modifier => modifier.kind === ts.SyntaxKind.DefaultKeyword) ? 'default' : node.name?.getText() ?? '(anonymous)'));
    }
  }
  return result;
}
function typeTargets(value) {
  if (!value || typeof value !== 'object') return [];
  return Object.entries(value).flatMap(([key, child]) => key === 'types' && typeof child === 'string' ? [child] : typeTargets(child));
}
function patternMatch(pattern, value) {
  const parts = pattern.split('*');
  if (parts.length === 1) return pattern === value ? '' : undefined;
  if (parts.length !== 2) throw new Error(`Unsupported export pattern: ${pattern}`);
  return value.startsWith(parts[0]) && value.endsWith(parts[1]) ? value.slice(parts[0].length, value.length - parts[1].length) : undefined;
}
const packages = manifests.map(manifest => {
  const directory = path.dirname(path.join(root, manifest));
  const pkg = JSON.parse(readFileSync(path.join(root, manifest), 'utf8'));
  const configFile = ['tsconfig.build.json', 'tsconfig.json'].map(name => path.join(directory, name)).find(existsSync);
  const config = configFile ? ts.getParsedCommandLineOfConfigFile(configFile, {}, {...ts.sys, onUnRecoverableConfigFileDiagnostic(error) {throw new Error(ts.flattenDiagnosticMessageText(error.messageText, '\n'));}}) : undefined;
  if (config?.errors.length) throw new Error(config.errors.map(error => ts.flattenDiagnosticMessageText(error.messageText, '\n')).join('\n'));
  const files = new Set();
  function visit(file) {
    if (files.has(file) || !file.startsWith(directory + path.sep) || file.includes(`${path.sep}node_modules${path.sep}`)) return;
    files.add(file);
    for (const imported of ts.preProcessFile(readFileSync(file, 'utf8'), true).importedFiles) {
      const target = resolveLocal(file, imported.fileName);
      if (target) visit(target);
    }
  }
  for (const file of config?.fileNames ?? []) visit(file);
  const sourceFiles = [...files].filter(file => !file.endsWith('.d.ts')).sort();
  let sourceRoot = config?.options.rootDir;
  if (!sourceRoot && sourceFiles.length) {
    sourceRoot = path.dirname(sourceFiles[0]);
    while (sourceFiles.some(file => !file.startsWith(sourceRoot + path.sep))) sourceRoot = path.dirname(sourceRoot);
  }
  const outputRoot = config?.options.declarationDir ?? config?.options.outDir;
  const emitted = new Map(sourceRoot && outputRoot ? sourceFiles.map(file => [path.relative(directory,
    path.join(outputRoot, path.relative(sourceRoot, file).replace(/\.(?:tsx?|mts)$/, '.d.ts'))).split(path.sep).join('/'), file]) : []);
  const entries = [];
  for (const [subpath, conditions] of Object.entries(pkg.exports ?? {})) {
    for (const target of typeTargets(conditions)) {
      const pattern = target.replace(/^\.\//, '');
      const matches = [...emitted].flatMap(([output, file]) => {
        const match = patternMatch(pattern, output);
        return match === undefined ? [] : [{subpath: subpath.replaceAll('*', match), target: output, source: relative(file)}];
      });
      entries.push(...(matches.length ? matches : [{subpath, target, unresolved: true}]));
    }
  }
  const publicFiles = new Set();
  function visitExports(file) {
    if (publicFiles.has(file)) return;
    publicFiles.add(file);
    for (const entry of exportsOf(file)) if (entry.kind === 'reexport' && entry.target) visitExports(path.join(root, entry.target));
  }
  for (const entry of entries) if (entry.source) visitExports(path.join(root, entry.source));
  return {name: pkg.name, manifest, private: pkg.private === true,
    bins: pkg.bin ?? {},
    entries: entries.sort((a, b) => a.subpath.localeCompare(b.subpath, 'en')),
    files: [...publicFiles].sort().map(file => ({source: relative(file), exports: exportsOf(file)})),
    // Private application modules are capabilities, not public npm export claims.
    ...(pkg.private ? {moduleSources: sourceFiles.map(relative)} : {})};
});
const report = {format: 1, scope: 'Source declarations and explicit reexport edges; external package exports, runtime schemas and semantic Rust mappings require review.', packages};
const destination = path.join(root, 'plan/rust-typescript-public-api.json');
const encoded = JSON.stringify(report, null, 2) + '\n';
if (process.argv.includes('--check')) {
  if (!existsSync(destination) || readFileSync(destination, 'utf8') !== encoded) throw new Error('Public API inventory is stale; run scripts/rust_api_inventory.mjs');
} else writeFileSync(destination, encoded);
const unresolved = packages.flatMap(pkg => pkg.entries.filter(entry => entry.unresolved).map(entry => `${pkg.name}:${entry.subpath}`));
console.log(JSON.stringify({packages: packages.length, entries: packages.reduce((sum, pkg) => sum + pkg.entries.length, 0),
  declarations: packages.reduce((sum, pkg) => sum + pkg.files.reduce((count, file) => count + file.exports.length, 0), 0), unresolved}, null, 2));
