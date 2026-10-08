#!/usr/bin/env python3
"""Actual CLI + owned HTTP fixture. No live account, package execution or publication."""
import json, os, shutil, subprocess, sys, tarfile, tempfile, threading, time
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
binary = Path(sys.argv[1]).resolve()
app_id = '00000000-0000-4000-8000-000000000001'
requests = []
status = 200
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        requests.append((self.path, self.headers.get('Authorization')))
        payload = json.dumps({'id': app_id} if self.path.startswith('/applications/') else {'isActive': True}).encode()
        self.send_response(status); self.send_header('Content-Length', str(len(payload))); self.end_headers(); self.wfile.write(payload)
    def do_POST(self):
        requests.append(('UNEXPECTED POST', None)); self.send_error(500)
server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp); app = root/'app'; home = root/'home'; bins = root/'bin'
        service = app/'src/modules/example'; service.mkdir(parents=True)
        (app/'.forklaunch/openapi/example').mkdir(parents=True)
        (home/'.forklaunch').mkdir(parents=True); bins.mkdir()
        (app/'.forklaunch/manifest.toml').write_text('''id = "test"
cli_version = "0.0.0"
app_name = "release-proof"
modules_path = "src/modules"
app_description = "Test"
linter = "eslint"
formatter = "prettier"
validator = "zod"
http_framework = "express"
runtime = "node"
author = "QA"
license = "MIT"
project_peer_topology = {}
[[projects]]
name = "example"
type = "Service"
description = "Test"
''')
        (service/'package.json').write_text('{"name":"@test/example","version":"1.0.0","scripts":{"build":"exit 91"}}')
        (service/'tsconfig.json').write_text('{}')
        (service/'server.ts').write_text('throw new Error("Application code must not execute");\n')
        spec_path = app/'.forklaunch/openapi/example/openapi.json'
        spec_path.write_text('{"openapi":"3.1.0","info":{"title":"Test","version":"1"},"paths":{}}')
        sentinel = root/'executed'
        for tool in ['pnpm','bun','node','npx','tsx']:
            path = bins/tool; path.write_text(f'#!/bin/sh\ntouch "{sentinel}"\nexit 99\n'); path.chmod(0o700)
        endpoint = f'http://127.0.0.1:{server.server_port}'
        env = {'HOME':str(home),'PATH':str(bins)+':/usr/bin:/bin','CI':'true','FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL':endpoint,'FORKLAUNCH_BILLING_API_URL':endpoint}
        args = [str(binary),'release','create','--local','--dry-run','--skip-sync','--yes','--application-id',app_id,'--prebuilt-openapi','.forklaunch/openapi','--version','1.0.0']
        manifest_path = app/'.forklaunch/release-manifest.json'
        def run(): return subprocess.run(args,cwd=app,env=env,capture_output=True,text=True,timeout=30)
        def login(expiry): (home/'.forklaunch/token').write_text(f'access_token = "synthetic-test-user-token"\nrefresh_token = ""\nexpires_at = {expiry}\n')
        for label in ['missing','expired']:
            if label == 'expired': login(1)
            result = run(); assert result.returncode != 0 and not manifest_path.exists(), label
        login(int(time.time())+300)
        status = 403
        result = run(); assert result.returncode != 0 and not manifest_path.exists(), 'Denied access must stop preparation'
        status = 200
        result = run(); assert result.returncode == 0, result.stdout+result.stderr
        manifest = json.loads(manifest_path.read_text()); assert manifest['applicationId'] == app_id
        assert not sentinel.exists(), 'Generated/package code executed'
        assert 'synthetic-test-user-token' not in manifest_path.read_text()
        assert any(path == '/applications/'+app_id and auth == 'Bearer synthetic-test-user-token' for path,auth in requests)
        assert not any('POST' in path for path,_ in requests), 'Dry-run published'
        manifest_path.unlink(); spec_path.unlink()
        assert run().returncode != 0 and not manifest_path.exists(), 'Missing specification accepted'
        if len(sys.argv) == 4:
            # Exercise the desktop's actual trusted program with the same compiled CLI.
            spec_path.write_text('{"openapi":"3.1.0","info":{"title":"Test","version":"1"},"paths":{}}')
            trusted = root/'trusted'; trusted.mkdir()
            shutil.copyfile(sys.argv[2], trusted/'prepare.mjs')
            shutil.copyfile(sys.argv[3], trusted/'scaffold-login.mjs')
            source = root/'built.tar'
            with tarfile.open(source, 'w') as archive:
                archive.add(app, arcname='.')
            for code in [200, 403]:
                status = code
                workspace = root/f'workspace-{code}'; workspace.mkdir()
                private_home = root/f'private-home-{code}'
                output = root/f'result-{code}.tar'
                options = {'application':app_id,'version':'1.0.0','login':{'accessToken':'synthetic-test-user-token','expiresAt':int(time.time())+300},'workspace':str(workspace),'home':str(private_home),'input':str(source),'output':str(output),'cli':str(binary)}
                entry = trusted/'test.mjs'
                entry.write_text('import { prepareRelease } from "./prepare.mjs"; await prepareRelease('+json.dumps(options)+');')
                result = subprocess.run([shutil.which('node'),str(entry)],env=env,capture_output=True,text=True,timeout=30)
                assert (result.returncode == 0) == (code == 200), result.stderr
                assert not (private_home/'.forklaunch/token').exists(), 'Desktop runner retained its login'
                assert output.exists() == (code == 200), 'Denied release emitted an artifact'
                if output.exists():
                    with tarfile.open(output) as archive:
                        for member in archive.getmembers():
                            assert not member.name.endswith('/token')
                            if member.isfile(): assert b'synthetic-test-user-token' not in archive.extractfile(member).read()
                assert not sentinel.exists(), 'Desktop runner executed package or generated code'
            print('PASS: actual desktop runner + actual CLI, access denied, token cleanup on success/failure, archive contains no credential, no generated code execution')
        print('PASS: missing/expired login, refused application, authenticated preparation, no generated execution, no credential in manifest, no publication, missing-spec rejection')
finally:
    server.shutdown(); server.server_close()
