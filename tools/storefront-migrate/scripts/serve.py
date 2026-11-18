#!/usr/bin/env python3
"""Static server for the local storefront mirror. Binds 127.0.0.1 only."""
import http.server, os, socketserver, sys

# usage: serve.py [PORT] [SITE_DIR]   (defaults: 4173, ./site next to this file)
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 4173
ROOT = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else \
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "site")
os.chdir(ROOT)


class H(http.server.SimpleHTTPRequestHandler):
    # Storefront routes are extensionless; map them onto the mirrored files.
    ROUTES = {
        "/": "/index.html",
        "/cart": "/cart.html",
    }

    def translate_path(self, path):
        clean = path.split("?", 1)[0].split("#", 1)[0]
        if clean in self.ROUTES:
            path = self.ROUTES[clean]
        elif not os.path.splitext(clean)[1] and clean != "/":
            # Extensionless storefront route (common on Hydrogen/React SPAs whose
            # client router rewrites the URL to e.g. /products/foo — with no
            # .html). On refresh or a direct link the STATIC server gets that
            # path, so resolve it to the real mirrored file: try <path>.html,
            # then <path>/index.html. Only if neither exists do we leave it as
            # given (a natural 404) — never a hardcoded wrong-product fallback,
            # which is what previously made every SPA product refresh serve the
            # same sample page.
            base = clean.rstrip("/")
            cand = base + ".html"
            if os.path.isfile(super().translate_path(cand)):
                path = cand
            else:
                idx = base + "/index.html"
                if os.path.isfile(super().translate_path(idx)):
                    path = idx
                else:
                    # urlmap.js folds deeper paths into one file:
                    # /blogs/recipes/<post> -> blogs/recipes-<post>.html.
                    # A direct URL to such a page must resolve the same way.
                    segs = [x for x in base.split("/") if x]
                    if 2 <= len(segs) <= 3:
                        fold = "/" + segs[0] + "/" + "-".join(segs[1:]) + ".html"
                        if os.path.isfile(super().translate_path(fold)):
                            path = fold
        return super().translate_path(path)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, fmt, *a):
        if "404" in (fmt % a):
            sys.stderr.write("404 %s\n" % (fmt % a))


socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(("127.0.0.1", PORT), H) as httpd:
    print(f"storefront mirror on http://127.0.0.1:{PORT}  (root={ROOT})", flush=True)
    httpd.serve_forever()
