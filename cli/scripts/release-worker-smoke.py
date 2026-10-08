#!/usr/bin/env python3
"""Local-only release proof. Uses synthetic credentials and an owned temporary fixture.

Usage: python3 cli/scripts/release-worker-smoke.py cli/target/debug/forklaunch
Requires Node and a candidate CLI built from this source. No packages are installed.
"""
import json
import base64
import hashlib
import hmac
import io
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import tarfile
import threading
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()
node = shutil.which('node')
assert node, 'Node is required'
with tempfile.TemporaryDirectory(prefix='fl-release-worker-smoke-') as temp:
    root = Path(temp)
    app = root / 'app'
    service = app / 'src/modules/example'
    shim = service / 'node_modules/tsx/dist/cli.mjs'
    shim.parent.mkdir(parents=True)
    (app / '.forklaunch').mkdir()
    home = root / 'home'
    home.mkdir()
    bins = root / 'bin'
    bins.mkdir()
    (bins / 'node').symlink_to(node)
    sentinel = root / 'package-ran'
    for tool in ('pnpm', 'bun'):
        path = bins / tool
        path.write_text(f'#!/bin/sh\nprintf ran > "{sentinel}"\nexit 99\n')
        path.chmod(0o700)
    (app / '.forklaunch/manifest.toml').write_text('''id = "local-test"
cli_version = "0.0.0"
app_name = "release-fixture"
modules_path = "src/modules"
app_description = "Synthetic release verification"
linter = "eslint"
formatter = "prettier"
validator = "zod"
http_framework = "express"
runtime = "node"
author = "QA"
license = "MIT"
platform_application_id = "00000000-0000-4000-8000-000000000001"
project_peer_topology = {}
[[projects]]
name = "example"
type = "Service"
description = "Fixture server"
''')
    (service / 'package.json').write_text(json.dumps({'name': '@fixture/example', 'version': '1.0.0', 'type': 'module'}))
    (service / 'tsconfig.json').write_text('{}')
    (service / 'server.ts').write_text('// Synthetic source fixture. Execution is asserted by the local runtime shim.\n')
    shim.write_text('''import fs from 'node:fs';
import assert from 'node:assert/strict';
for (const key of ['FORKLAUNCH_HMAC_SECRET', 'AZURE_API_KEY', 'AWS_SECRET_ACCESS_KEY', 'NODE_OPTIONS']) {
  assert.equal(process.env[key], undefined, `Credential or hook inherited: ${key}`);
}
assert.equal(process.env.FORKLAUNCH_MODE, 'openapi');
assert.ok(fs.existsSync(`${process.env.HOME}/.npmrc`));
assert.ok(!fs.existsSync(`${process.env.HOME}/.codex/auth.json`));
fs.writeFileSync(process.env.FORKLAUNCH_OPENAPI_OUTPUT, JSON.stringify({openapi:'3.1.0',info:{title:'Fixture',version:'1.0.0'},paths:{}}));
fs.writeFileSync('export-proof.json', JSON.stringify({home:process.env.HOME}));
''')
    env = {'PATH': str(bins) + ':/usr/bin:/bin', 'HOME': str(home), 'CI': 'true',
           'FORKLAUNCH_HMAC_SECRET': 'synthetic-release-secret',
           'AZURE_API_KEY': 'synthetic-provider-secret', 'AWS_SECRET_ACCESS_KEY': 'synthetic-cloud-secret',
           'FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL': 'http://127.0.0.1:1'}
    result = subprocess.run([str(binary), 'release', 'create', '--version', '1.0.0', '--local', '--yes',
                             '--skip-sync', '--skip-package-build', '--dry-run'],
                            cwd=app, env=env, capture_output=True, text=True, timeout=90)
    assert result.returncode == 0, result.stdout + result.stderr
    assert not sentinel.exists(), 'Package script ran despite --skip-package-build'
    manifest = json.loads((app / '.forklaunch/release-manifest.json').read_text())
    assert manifest, 'Missing release manifest'
    proof = json.loads((service / 'export-proof.json').read_text())
    assert not Path(proof['home']).exists(), 'Export home leaked after completion'
    assert not (app / '.forklaunch/openapi').exists(), 'Export files leaked after completion'
    print('PASS: actual CLI dry-run exported a service, wrote a release manifest, skipped package scripts, stripped credentials, and cleaned export files/home. No platform deployment or upload was attempted.')

    requests = []
    uploaded = {}
    failures = []
    fault = None

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def respond(self, body, status=200):
            payload = json.dumps(body).encode()
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self):
            try:
                raw = self.rfile.read(int(self.headers['Content-Length']))
                body = json.loads(raw)
                auth = self.headers.get('Authorization', '')
                assert auth.startswith('HMAC '), 'Missing request signature'
                fields = dict(part.split('=', 1) for part in auth[5:].split(' '))
                sign_path = self.path.removeprefix('/releases')
                canonical = json.dumps(body, separators=(',', ':'), ensure_ascii=False)
                message = f"POST\n{sign_path}\n{canonical}\n{fields['ts']}\n{fields['nonce']}"
                expected = base64.b64encode(hmac.new(b'synthetic-release-secret', message.encode(), hashlib.sha256).digest()).decode()
                assert hmac.compare_digest(fields['signature'], expected), 'Invalid request signature'
                requests.append((self.path, body))
                base = f'http://127.0.0.1:{self.server.server_port}'
                if self.path == '/releases/internal/upload-url':
                    self.respond({'uploadUrl': base + '/upload/code', 'codeSourceUrl': base + '/stored/code'})
                elif self.path == '/releases/internal/openapi-upload-urls':
                    self.respond({'urls': {} if fault == 'missing-spec-url' else {'example': {'uploadUrl': base + '/upload/openapi', 's3Key': 'fixture/example.json'}}})
                elif self.path == '/releases/internal':
                    assert '/upload/code' in uploaded and '/upload/openapi' in uploaded
                    self.respond({'id': 'synthetic-release'})
                else:
                    raise AssertionError('Unexpected route ' + self.path)
            except Exception as error:
                failures.append(str(error))
                self.respond({'error': 'fixture assertion failed'}, 400)

        def do_PUT(self):
            if self.path not in ('/upload/code', '/upload/openapi'):
                failures.append('Unexpected upload route')
                self.respond({}, 400)
                return
            assert self.headers.get('Authorization') is None, 'Release credential forwarded to upload URL'
            payload = self.rfile.read(int(self.headers['Content-Length']))
            if fault == 'upload-failure':
                self.respond({'error': 'simulated storage failure'}, 503)
                return
            uploaded[self.path] = payload
            self.respond({})

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        env['FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL'] = f'http://127.0.0.1:{server.server_port}'
        result = subprocess.run([str(binary), 'release', 'create', '--version', '1.0.1', '--local', '--yes',
                                 '--skip-sync', '--skip-package-build'],
                                cwd=app, env=env, capture_output=True, text=True, timeout=90)
        assert result.returncode == 0, result.stdout + result.stderr + repr(failures)
        assert not failures, failures
        assert [path for path, body in requests] == ['/releases/internal/upload-url', '/releases/internal/openapi-upload-urls', '/releases/internal']
        assert not sentinel.exists(), 'Package script ran during authenticated release'
        with tarfile.open(fileobj=io.BytesIO(uploaded['/upload/code']), mode='r:gz') as archive:
            names = archive.getnames()
            assert 'src/modules/example/server.ts' in names
            assert all('node_modules' not in name for name in names)
        assert json.loads(uploaded['/upload/openapi'])['v1']['openapi'] == '3.1.0'
        assert not (app / '.forklaunch/release-code.tar.gz').exists()
        assert not (app / '.forklaunch/openapi').exists()
        assert not Path(json.loads((service / 'export-proof.json').read_text())['home']).exists()
        print('PASS: actual CLI packaged code, uploaded code/specification, and created a release against a local fixture. All three request signatures verified; upload URLs received no release credential. This is not a live platform or cloud deployment.')
        for fault in ('upload-failure', 'missing-spec-url'):
            requests.clear()
            uploaded.clear()
            result = subprocess.run([str(binary), 'release', 'create', '--version', '1.0.2', '--local', '--yes',
                                     '--skip-sync', '--skip-package-build'],
                                    cwd=app, env=env, capture_output=True, text=True, timeout=90)
            assert result.returncode != 0, 'Release incorrectly succeeded: ' + fault
            assert not any(path == '/releases/internal' for path, body in requests), 'Incomplete release submitted'
            assert not list((app / '.forklaunch').glob('release-code-*')), 'Release archive leaked on failure'
            assert not (app / '.forklaunch/openapi').exists(), 'OpenAPI files leaked on failure'
            assert not Path(json.loads((service / 'export-proof.json').read_text())['home']).exists()
            assert not sentinel.exists()
            assert not failures, failures
        print('PASS: failed storage upload and missing API-specification upload URL both stop release creation and clean temporary files.')
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
