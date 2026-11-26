// Only framework-created diagnostics use this synchronous, per-response scope.
// Application output still passes through its normal response contract checks.
const sendingDiagnostics = new WeakSet<object>();

export function isValidationDiagnostic(response: object): boolean {
  return sendingDiagnostics.has(response);
}

export function sendValidationDiagnostic<T>(
  response: object,
  send: () => T
): T {
  const alreadySending = sendingDiagnostics.has(response);
  sendingDiagnostics.add(response);
  try {
    return send();
  } finally {
    if (!alreadySending) sendingDiagnostics.delete(response);
  }
}
