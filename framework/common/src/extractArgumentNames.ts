/**
 * Extracts the names of arguments from a function's string representation.
 * This is useful for reflection and debugging purposes.
 *
 * @param {Object} func - A function or object with a toString method that returns the function definition
 * @returns {string[]} An array of argument names
 * @example
 * function example(a, b, { c, d }) {}
 * const names = extractArgumentNames(example);
 * // Result: ['a', 'b', '{c,d}']
 */
export function extractArgumentNames(func: { toString(): string }): string[] {
  const fnStr = func.toString();
  const parameterList = findParameterList(fnStr);
  if (parameterList == null) return [];

  const argsStr = fnStr.slice(parameterList.start + 1, parameterList.end);
  const result = [];
  let currentArg = '';
  let braceCount = 0;
  let bracketCount = 0;
  let parenCount = 0;

  for (let i = 0; i < argsStr.length; i++) {
    const char = argsStr[i];

    if (char === '{') braceCount++;
    if (char === '}') braceCount--;
    if (char === '[') bracketCount++;
    if (char === ']') bracketCount--;
    if (char === '(') parenCount++;
    if (char === ')') parenCount--;

    if (
      char === ',' &&
      braceCount === 0 &&
      bracketCount === 0 &&
      parenCount === 0
    ) {
      result.push(normalizeArgument(currentArg));
      currentArg = '';
    } else {
      currentArg += char;
    }
  }

  if (currentArg.trim()) {
    result.push(normalizeArgument(currentArg));
  }

  return result;
}

function findParameterList(
  fnStr: string
): { start: number; end: number } | undefined {
  for (let i = 0; i < fnStr.length; i++) {
    if (fnStr[i] !== '(') continue;

    const end = findMatchingParen(fnStr, i);
    if (end === -1) continue;

    if (isParameterList(fnStr, end)) {
      return { start: i, end };
    }
  }
}

function isParameterList(fnStr: string, closingParenIndex: number): boolean {
  let nextIndex = skipWhitespace(fnStr, closingParenIndex + 1);
  if (fnStr.startsWith('=>', nextIndex)) return true;
  if (fnStr[nextIndex] === '{') return true;

  if (fnStr[nextIndex] !== ':') return false;

  nextIndex++;
  let braceCount = 0;
  let bracketCount = 0;
  let parenCount = 0;

  for (let i = nextIndex; i < fnStr.length - 1; i++) {
    const char = fnStr[i];

    if (char === '{') braceCount++;
    if (char === '}') braceCount--;
    if (char === '[') bracketCount++;
    if (char === ']') bracketCount--;
    if (char === '(') parenCount++;
    if (char === ')') parenCount--;

    if (
      braceCount === 0 &&
      bracketCount === 0 &&
      parenCount === 0 &&
      fnStr.startsWith('=>', i)
    ) {
      return true;
    }
  }

  return false;
}

function findMatchingParen(fnStr: string, openingParenIndex: number): number {
  let depth = 0;
  let quote: string | undefined;
  let escaped = false;

  for (let i = openingParenIndex; i < fnStr.length; i++) {
    const char = fnStr[i];

    if (quote != null) {
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === quote) {
        quote = undefined;
      }
      continue;
    }

    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      continue;
    }

    if (char === '(') depth++;
    if (char === ')') {
      depth--;
      if (depth === 0) return i;
    }
  }

  return -1;
}

function skipWhitespace(value: string, startIndex: number): number {
  let index = startIndex;
  while (/\s/.test(value[index] ?? '')) index++;
  return index;
}

function normalizeArgument(arg: string): string {
  return stripTopLevelTypeAnnotation(arg).replace(/\s+/g, '');
}

function stripTopLevelTypeAnnotation(arg: string): string {
  let braceCount = 0;
  let bracketCount = 0;
  let parenCount = 0;

  for (let i = 0; i < arg.length; i++) {
    const char = arg[i];

    if (char === '{') braceCount++;
    if (char === '}') braceCount--;
    if (char === '[') bracketCount++;
    if (char === ']') bracketCount--;
    if (char === '(') parenCount++;
    if (char === ')') parenCount--;

    if (
      char === ':' &&
      braceCount === 0 &&
      bracketCount === 0 &&
      parenCount === 0
    ) {
      return arg.slice(0, i);
    }
  }

  return arg;
}
