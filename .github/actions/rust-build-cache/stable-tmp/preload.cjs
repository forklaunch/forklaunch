// Deterministic fs.mkdtemp names for CI test scripts; see sitecustomize.py
// beside this file. Loaded through NODE_OPTIONS=--require when
// FORKLAUNCH_STABLE_TMP=1. Each call still creates a new, empty directory.
'use strict';
if (process.env.FORKLAUNCH_STABLE_TMP === '1') {
  const fs = require('node:fs');
  const { syncBuiltinESMExports } = require('node:module');
  const original = fs.mkdtempSync;
  const next = new Map();
  const isBuffer = (options) => options === 'buffer' || (options && options.encoding === 'buffer');
  const stable = function mkdtempSync(prefix, options) {
    if (typeof prefix !== 'string' || isBuffer(options)) return original.apply(fs, arguments);
    let number = next.get(prefix) || 0;
    for (;;) {
      const path = `${prefix}fl${String(number).padStart(4, '0')}`;
      number += 1;
      try {
        fs.mkdirSync(path, { mode: 0o700 });
      } catch (error) {
        if (error && error.code === 'EEXIST') continue;
        throw error;
      }
      next.set(prefix, number);
      return path;
    }
  };
  const originalAsync = fs.mkdtemp;
  fs.mkdtempSync = stable;
  fs.mkdtemp = function mkdtemp(prefix, options, callback) {
    if (typeof options === 'function') { callback = options; options = undefined; }
    if (typeof prefix !== 'string' || isBuffer(options)) return originalAsync.apply(fs, arguments);
    let result;
    try { result = stable(prefix, options); } catch (error) { process.nextTick(callback, error); return; }
    process.nextTick(callback, null, result);
  };
  const originalPromise = fs.promises.mkdtemp;
  fs.promises.mkdtemp = async function mkdtemp(prefix, options) {
    if (typeof prefix !== 'string' || isBuffer(options)) return originalPromise.apply(fs.promises, arguments);
    return stable(prefix, options);
  };
  syncBuiltinESMExports();
}
