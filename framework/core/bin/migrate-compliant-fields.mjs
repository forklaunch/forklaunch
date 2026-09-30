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
 * Usage: npx -y -p typescript@5 -p @forklaunch/core forklaunch-migrate-compliant-fields
 *          [path/to/tsconfig.json] [--dry-run] [--typescript <path to a typescript 5 package>]
 *          [--import-from <module exporting deanon>]
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

const args = process.argv.slice(2);
const dryRun = args.includes('--dry-run');
const positional = args.filter(
  (a, i) =>
    !a.startsWith('--') &&
    args[i - 1] !== '--typescript' &&
    args[i - 1] !== '--import-from'
);
const tsconfigPath = path.resolve(positional[0] ?? 'tsconfig.json');
const importFrom =
  args.includes('--import-from')
    ? args[args.indexOf('--import-from') + 1]
    : '@forklaunch/core/persistence';

// The codemod needs TypeScript's JavaScript compiler API, which TypeScript 7
// (the native compiler) does not ship. Use, in order: an explicit
// --typescript <module>, a TypeScript next to this script (npx -p
// typescript@5 ...), then the project's own.
function loadTypeScript() {
  const explicit = args[args.indexOf('--typescript') + 1];
  const candidates = [
    args.includes('--typescript') && explicit
      ? () => createRequire(path.resolve(explicit, 'package.json'))('./')
      : null,
    () => createRequire(import.meta.url)('typescript'),
    () => createRequire(tsconfigPath)('typescript')
  ].filter(Boolean);
  for (const load of candidates) {
    try {
      const candidate = load();
      if (candidate?.sys && candidate?.createProgram) return candidate;
    } catch {
      // try the next one
    }
  }
  console.error(
    'This codemod needs TypeScript 5 (TypeScript 7 has no JavaScript compiler API). Run it as:\n' +
      '  npx -y -p typescript@5 -p @forklaunch/core forklaunch-migrate-compliant-fields [tsconfig.json]'
  );
  process.exit(1);
}
const ts = loadTypeScript();

const READ_MEMBERS = new Set(['anon', 'deanon', 'level']);

function loadProgram() {
  const config = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
  if (config.error) {
    console.error(ts.flattenDiagnosticMessageText(config.error.messageText, '\n'));
    process.exit(1);
  }
  const parsed = ts.parseJsonConfigFileContent(
    config.config,
    ts.sys,
    path.dirname(tsconfigPath)
  );
  return ts.createProgram(parsed.fileNames, parsed.options);
}

function isCompliantType(checker, type) {
  if (!type) return false;
  if (type.isUnion()) {
    return type.types.some((t) => isCompliantType(checker, t));
  }
  const symbol = type.aliasSymbol ?? type.getSymbol();
  return symbol?.getName() === 'CompliantField';
}

function isNullableCompliant(type) {
  return (
    type.isUnion() &&
    type.types.some(
      (t) => t.flags & (ts.TypeFlags.Null | ts.TypeFlags.Undefined)
    )
  );
}

/** Does `target` accept a CompliantField as it is (entity data, where, any)? */
function acceptsField(checker, source, target) {
  if (!target) return true;
  if (target.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)) return true;
  if (typeof checker.isTypeAssignableTo === 'function') {
    return checker.isTypeAssignableTo(source, target);
  }
  return isCompliantType(checker, target);
}

/** Why this expression, typed CompliantField, needs its value. */
function needsValue(checker, node, type) {
  const parent = node.parent;
  if (!parent) return false;

  // user.email.toLowerCase()  (anything but .anon / .deanon / .level)
  if (
    ts.isPropertyAccessExpression(parent) &&
    parent.expression === node &&
    !READ_MEMBERS.has(parent.name.text)
  ) {
    return true;
  }
  // `${user.email}`
  if (ts.isTemplateSpan(parent) && parent.expression === node) return true;
  // user.email === input, user.email + '!'
  if (ts.isBinaryExpression(parent)) {
    const op = parent.operatorToken.kind;
    const other = parent.left === node ? parent.right : parent.left;
    const comparison = [
      ts.SyntaxKind.EqualsEqualsEqualsToken,
      ts.SyntaxKind.ExclamationEqualsEqualsToken,
      ts.SyntaxKind.EqualsEqualsToken,
      ts.SyntaxKind.ExclamationEqualsToken,
      ts.SyntaxKind.PlusToken
    ].includes(op);
    if (comparison) {
      const otherType = checker.getTypeAtLocation(other);
      const otherIsNullish =
        otherType.flags & (ts.TypeFlags.Null | ts.TypeFlags.Undefined);
      return !isCompliantType(checker, otherType) && !otherIsNullish;
    }
    if (op === ts.SyntaxKind.EqualsToken && parent.left === node) return false;
  }
  // Assignments, returns, arguments, properties: compare with the
  // contextual type.
  const contextual = checker.getContextualType(node);
  if (contextual) return !acceptsField(checker, type, contextual);
  return false;
}

function collectEdits(program) {
  const checker = program.getTypeChecker();
  const edits = new Map(); // fileName -> [{ pos, text }]
  let count = 0;

  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile) continue;
    if (sourceFile.fileName.includes('/node_modules/')) continue;
    const fileEdits = [];
    let needsImport = false;

    const visit = (node) => {
      // { ...entity }: compliant properties live on the prototype, so a
      // spread drops them; deanon(entity) copies them as values.
      if (ts.isSpreadAssignment(node)) {
        const spreadType = checker.getTypeAtLocation(node.expression);
        const carriesField = checker
          .getPropertiesOfType(spreadType)
          .some((property) =>
            isCompliantType(
              checker,
              checker.getTypeOfSymbolAtLocation(property, node.expression)
            )
          );
        if (carriesField) {
          fileEdits.push({ pos: node.expression.getStart(), text: 'deanon(' });
          fileEdits.push({ pos: node.expression.getEnd(), text: ')' });
          needsImport = true;
          count += 1;
          return;
        }
      }
      const isCandidate =
        ts.isPropertyAccessExpression(node) ||
        ts.isElementAccessExpression(node) ||
        (ts.isIdentifier(node) &&
          !ts.isPropertyAccessExpression(node.parent) &&
          !ts.isDeclaration(node.parent));
      if (isCandidate) {
        const type = checker.getTypeAtLocation(node);
        if (isCompliantType(checker, type) && needsValue(checker, node, type)) {
          fileEdits.push({
            pos: node.getEnd(),
            text: isNullableCompliant(type) ? '?.deanon' : '.deanon'
          });
          count += 1;
          return; // do not descend into a rewritten expression
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);

    if (needsImport && !/\bdeanon\b[^;]*from/.test(sourceFile.text)) {
      const imports = sourceFile.statements.filter(ts.isImportDeclaration);
      const pos = imports.length ? imports[imports.length - 1].getEnd() : 0;
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

function reportRemaining(program) {
  const remaining = ts
    .getPreEmitDiagnostics(program)
    .filter((d) =>
      ts.flattenDiagnosticMessageText(d.messageText, '\n').includes('CompliantField')
    );
  if (remaining.length === 0) {
    console.log('No compliant-field type errors remain.');
    return 0;
  }
  console.log(`\n${remaining.length} compliant-field error(s) need a decision:`);
  for (const d of remaining) {
    const where = d.file
      ? (() => {
          const { line, character } = d.file.getLineAndCharacterOfPosition(d.start ?? 0);
          return `${path.relative(process.cwd(), d.file.fileName)}:${line + 1}:${character + 1}`;
        })()
      : '';
    const message = ts.flattenDiagnosticMessageText(d.messageText, ' ');
    const hint = /FilterQuery|FilterValue|ExpandScalar|OperatorMap/.test(message)
      ? '  -> a `where` on a field that is not queryable: add `{ queryable: true }` to its .compliance() call and generate a migration'
      : '';
    console.log(`  ${where}  ${message.slice(0, 240)}${hint ? `\n${hint}` : ''}`);
  }
  return remaining.length;
}

let program = loadProgram();
const { edits, count } = collectEdits(program);
applyEdits(edits);
console.log(
  `${count} read(s) ${dryRun ? 'would be' : ''} rewritten to .deanon across ${edits.size} file(s).`
);
if (!dryRun && count > 0) program = loadProgram();
const left = reportRemaining(program);
console.log(
  '\nNext: generate a migration (queryable fields add <column>_idx), deploy, then run ' +
    'reencryptEncryptedColumns() once to seal existing rows into v4 and write blind indexes.'
);
process.exit(left > 0 ? 2 : 0);
