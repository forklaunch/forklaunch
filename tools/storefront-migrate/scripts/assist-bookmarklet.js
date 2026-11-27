/**
 * Assisted-capture bookmarklet SOURCE.
 *
 * Build the bookmark: minify the IIFE below to one line and prefix it with
 * `javascript:` — `node scripts/make-bookmarklet.mjs` prints the ready URL.
 * The operator saves it as a bookmark, then on each page they want captured
 * they click it. The click is a genuine user gesture, so Chrome performs the
 * download (gesture-less programmatic downloads are blocked; a bookmarklet
 * click is not). No network request leaves the page, so store CSP is
 * irrelevant. The file is named so `import-folder.mjs` can recover the path.
 *
 * This is operator-driven, consensual capture of public pages — the same as
 * a person using File > Save As, just one click and correctly named.
 */
(function () {
  var html = '<!DOCTYPE html>' + document.documentElement.outerHTML;
  // Encode the pathname into the filename: "/products/x" -> "flcap__products__x.html".
  // import-folder.mjs reverses this. Query/hash dropped (not part of the page identity).
  var p = location.pathname.replace(/\/+$/, '') || '/index';
  var name = 'flcap' + p.replace(/[^A-Za-z0-9._/-]/g, '_').replace(/\//g, '__') + '.html';
  var blob = new Blob([html], { type: 'text/html' });
  var a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(function () { URL.revokeObjectURL(a.href); }, 4000);
})();
