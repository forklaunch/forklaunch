#!/usr/bin/env node
/**
 * Codemod for @forklaunch/core compliant fields.
 *
 * pii, phi and pci properties now load as CompliantField: the value is read
 * with `.deanon` (plaintext) or `.anon` (de-identified). This rewrites every
 * place a project used such a property as its value, by asking the
 * project's own TypeScript program where a CompliantField meets something
 * that is not one:
 *
 *   const s: string = user.email          ->  user.email.deanon
 *   return { to: record.to }              ->  record.to.deanon
 *   send(record.body)                     ->  send(record.body.deanon)
 *   `Hi ${user.name}`                     ->  `Hi ${user.name.deanon}`
 *   user.email.toLowerCase()              ->  user.email.deanon.toLowerCase()
 *   user.email === input                  ->  user.email.deanon === input
 *   ({ ...entity })                       ->  ({ ...deanon(entity) })
 *
 * Nullable fields get `?.deanon`. Entity data (em.create, em.assign) and
 * `where` clauses are left alone: they accept plain values already.
 *
 * What it cannot decide is reported, not guessed: chiefly `where` clauses on
 * fields that are not queryable, which need `.compliance(level, { queryable:
 * true })` on the property (and a migration adding its `<column>_idx`).
 *
 * It runs on TypeScript 7's API (`typescript/unstable/sync`, the native
 * compiler's type checker), from the project's own TypeScript when it has
 * one, else the one installed with @forklaunch/core.
 *
 * Usage: npx -y -p @forklaunch/core forklaunch-migrate-compliant-fields
 *          [path/to/tsconfig.json] [--dry-run] [--import-from <module exporting deanon>]
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
const dryRun = args.includes('--dry-run');
const positional = args.filter(
  (a, i) => !a.startsWith('--') && args[i - 1] !== '--import-from'
);
const tsconfigPath = path.resolve(positional[0] ?? 'tsconfig.json');
const importFrom = args.includes('--import-from')
  ? args[args.indexOf('--import-from') + 1]
  : '@forklaunch/core/persistence';

/** A TypeScript 7 module: the project's own first, then ours. */
async function loadTypeScript(subpath) {
  for (const from of [tsconfigPath, import.meta.url]) {
    try {
      const resolved = createRequire(from).resolve(`typescript/${subpath}`);
      return await import(pathToFileURL(resolved).href);
    } catch {
      // try the next one
    }
  }
  console.error(
    `This codemod needs TypeScript 7 (typescript/${subpath}). Install typescript@7 in the project and run it again.`
  );
  process.exit(1);
}
const { API, TypeFlags } = await loadTypeScript('unstable/sync');
const ast = await loadTypeScript('unstable/ast');
const { SyntaxKind } = ast;

const READ_MEMBERS = new Set(['anon', 'deanon', 'level']);

/** The kinds TypeScript counts as declarations (ts.isDeclarationKind). */
const DECLARATION_KINDS = new Set(
  [
    'ArrowFunction', 'BindingElement', 'ClassDeclaration', 'ClassExpression',
    'ClassStaticBlockDeclaration', 'Constructor', 'EnumDeclaration', 'EnumMember',
    'ExportSpecifier', 'FunctionDeclaration', 'FunctionExpression', 'GetAccessor',
    'ImportClause', 'ImportEqualsDeclaration', 'ImportSpecifier', 'InterfaceDeclaration',
    'JsxAttribute', 'MethodDeclaration', 'MethodSignature', 'ModuleDeclaration',
    'NamespaceExportDeclaration', 'NamespaceImport', 'NamespaceExport', 'Parameter',
    'PropertyAssignment', 'PropertyDeclaration', 'PropertySignature', 'SetAccessor',
    'ShorthandPropertyAssignment', 'TypeAliasDeclaration', 'TypeParameter',
    'VariableDeclaration'
  ]
    .map((name) => SyntaxKind[name])
    .filter((kind) => kind !== undefined)
);
const isDeclaration = (node) => Boolean(node) && DECLARATION_KINDS.has(node.kind);

/** Open the project; a fresh API per load so edits on disk are re-read. */
function loadProject() {
  const api = new API({ cwd: path.dirname(tsconfigPath) });
  const snapshot = api.updateSnapshot({ openProjects: [tsconfigPath] });
  const project = snapshot.getProject(tsconfigPath) ?? snapshot.getProjects()[0];
  if (!project) {
    console.error(`Could not open ${tsconfigPath}`);
    api.close();
    process.exit(1);
  }
  return { api, project };
}

const isUnion = (type) => Boolean(type && type.flags & TypeFlags.Union);

function isCompliantType(type) {
  if (!type) return false;
  if (isUnion(type)) return type.getTypes().some(isCompliantType);
  const symbol = type.getAliasSymbol() ?? type.getSymbol();
  return symbol?.name === 'CompliantField';
}

function isNullableCompliant(type) {
  return (
    isUnion(type) &&
    type.getTypes().some((t) => t.flags & (TypeFlags.Null | TypeFlags.Undefined))
  );
}

/** Does `target` accept a CompliantField as it is (entity data, where, any)? */
function acceptsField(checker, source, target) {
  if (!target) return true;
  if (target.flags & (TypeFlags.Any | TypeFlags.Unknown)) return true;
  return checker.isTypeAssignableTo(source, target);
}

/** Why this expression, typed CompliantField, needs its value. */
function needsValue(checker, node, type) {
  const parent = node.parent;
  if (!parent) return false;

  // user.email.toLowerCase()  (anything but .anon / .deanon / .level)
  if (
    ast.isPropertyAccessExpression(parent) &&
    parent.expression === node &&
    !READ_MEMBERS.has(parent.name.text)
  ) {
    return true;
  }
  // `${user.email}`
  if (ast.isTemplateSpan(parent) && parent.expression === node) return true;
  // user.email === input, user.email + '!'
  if (ast.isBinaryExpression(parent)) {
    const op = parent.operatorToken.kind;
    const other = parent.left === node ? parent.right : parent.left;
    const comparison = [
      SyntaxKind.EqualsEqualsEqualsToken,
      SyntaxKind.ExclamationEqualsEqualsToken,
      SyntaxKind.EqualsEqualsToken,
      SyntaxKind.ExclamationEqualsToken,
      SyntaxKind.PlusToken
    ].includes(op);
    if (comparison) {
      const otherType = checker.getTypeAtLocation(other);
      const otherIsNullish =
        otherType && otherType.flags & (TypeFlags.Null | TypeFlags.Undefined);
      return !isCompliantType(otherType) && !otherIsNullish;
    }
    if (op === SyntaxKind.EqualsToken && parent.left === node) return false;
  }
  // Assignments, returns, arguments, properties: compare with the
  // contextual type.
  const contextual = checker.getContextualType(node);
  if (contextual) return !acceptsField(checker, type, contextual);
  return false;
}

function sourceFiles(program) {
  return program
    .getSourceFileNames()
    .filter((fileName) => !fileName.includes('/node_modules/'))
    .map((fileName) => program.getSourceFile(fileName))
    .filter((sourceFile) => sourceFile && !sourceFile.isDeclarationFile);
}

function collectEdits(project) {
  const { checker, program } = project;
  const edits = new Map(); // fileName -> [{ pos, text }]
  let count = 0;

  for (const sourceFile of sourceFiles(program)) {
    const fileEdits = [];
    let needsImport = false;

    const visit = (node) => {
      // { ...entity }: compliant properties live on the prototype, so a
      // spread drops them; deanon(entity) copies them as values.
      if (ast.isSpreadAssignment(node)) {
        const spreadType = checker.getTypeAtLocation(node.expression);
        const carriesField =
          spreadType &&
          checker
            .getPropertiesOfType(spreadType)
            .some((property) =>
              isCompliantType(
                checker.getTypeOfSymbolAtLocation(property, node.expression)
              )
            );
        if (carriesField) {
          fileEdits.push({
            pos: ast.getTokenPosOfNode(node.expression, sourceFile),
            text: 'deanon('
          });
          fileEdits.push({ pos: node.expression.end, text: ')' });
          needsImport = true;
          count += 1;
          return undefined;
        }
      }
      const isCandidate =
        ast.isPropertyAccessExpression(node) ||
        ast.isElementAccessExpression(node) ||
        (ast.isIdentifier(node) &&
          !ast.isPropertyAccessExpression(node.parent) &&
          !isDeclaration(node.parent));
      if (isCandidate) {
        const type = checker.getTypeAtLocation(node);
        if (isCompliantType(type) && needsValue(checker, node, type)) {
          fileEdits.push({
            pos: node.end,
            text: isNullableCompliant(type) ? '?.deanon' : '.deanon'
          });
          count += 1;
          return undefined; // do not descend into a rewritten expression
        }
      }
      node.forEachChild(visit);
      return undefined; // a truthy return would stop forEachChild
    };
    visit(sourceFile);

    if (needsImport && !/\bdeanon\b[^;]*from/.test(sourceFile.text)) {
      const imports = sourceFile.statements.filter(ast.isImportDeclaration);
      const pos = imports.length ? imports[imports.length - 1].end : 0;
      fileEdits.push({
        pos,
        text: `${pos ? '\n' : ''}import { deanon } from '${importFrom}';${pos ? '' : '\n'}`
      });
    }
    if (fileEdits.length) edits.set(sourceFile.fileName, fileEdits);
  }
  return { edits, count };
}

function applyEdits(edits) {
  for (const [fileName, fileEdits] of edits) {
    let text = readFileSync(fileName, 'utf8');
    // Apply back to front; at one position, later-pushed edits go first so
    // pushes read left to right in the result.
    const ordered = fileEdits
      .map((edit, index) => ({ ...edit, index }))
      .sort((a, b) => b.pos - a.pos || b.index - a.index);
    for (const edit of ordered) {
      text = text.slice(0, edit.pos) + edit.text + text.slice(edit.pos);
    }
    if (!dryRun) writeFileSync(fileName, text);
    console.log(
      `${dryRun ? 'would edit' : 'edited'} ${path.relative(process.cwd(), fileName)} (${fileEdits.length})`
    );
  }
}

/** A diagnostic's message and its chain, flattened. */
function flatten(diagnostic, separator) {
  return [diagnostic.text, ...(diagnostic.messageChain ?? []).map((d) => flatten(d, separator))]
    .filter(Boolean)
    .join(separator);
}

function reportRemaining(project) {
  const { program } = project;
  const remaining = sourceFiles(program)
    .flatMap((sourceFile) => [
      ...program.getSyntacticDiagnostics(sourceFile.fileName),
      ...program.getSemanticDiagnostics(sourceFile.fileName)
    ])
    .filter((d) => flatten(d, '\n').includes('CompliantField'));
  if (remaining.length === 0) {
    console.log('No compliant-field type errors remain.');
    return 0;
  }
  console.log(`\n${remaining.length} compliant-field error(s) need a decision:`);
  for (const d of remaining) {
    const sourceFile = d.fileName ? program.getSourceFile(d.fileName) : undefined;
    const where = sourceFile
      ? (() => {
          const { line, character } = sourceFile.getLineAndCharacterOfPosition(d.pos ?? 0);
          return `${path.relative(process.cwd(), d.fileName)}:${line + 1}:${character + 1}`;
        })()
      : '';
    const message = flatten(d, ' ');
    const hint = /FilterQuery|FilterValue|ExpandScalar|OperatorMap/.test(message)
      ? '  -> a `where` on a field that is not queryable: add `{ queryable: true }` to its .compliance() call and generate a migration'
      : '';
    console.log(`  ${where}  ${message.slice(0, 240)}${hint ? `\n${hint}` : ''}`);
  }
  return remaining.length;
}

let loaded = loadProject();
const { edits, count } = collectEdits(loaded.project);
applyEdits(edits);
console.log(
  `${count} read(s) ${dryRun ? 'would be' : ''} rewritten to .deanon across ${edits.size} file(s).`
);
if (!dryRun && count > 0) {
  loaded.api.close();
  loaded = loadProject();
}
const left = reportRemaining(loaded.project);
loaded.api.close();
console.log(
  '\nNext: generate a migration (queryable fields add <column>_idx), deploy, then run ' +
    'reencryptEncryptedColumns() once to seal existing rows into v4 and write blind indexes.'
);
process.exit(left > 0 ? 2 : 0);
