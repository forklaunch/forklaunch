#!/usr/bin/env python3
"""Exercise native name prompts through a real terminal, and noninteractive failure."""
import errno
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import tempfile
import time
import tomllib

repo = Path(__file__).resolve().parents[3]
binary = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
env = {**os.environ, 'FORKLAUNCH_ACCOUNT': 'parity-offline-name-prompts', 'FORKLAUNCH_SKIP_FORMAT': '1'}

def terminal(args, expected, answer):
    pid, descriptor = pty.fork()
    if pid == 0:
        os.execve(binary, [str(binary), *args], env)
    output = b''
    answered = False
    completed = False
    try:
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if select.select([descriptor], [], [], 0.1)[0]:
                try:
                    data = os.read(descriptor, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    data = b''
                output += data
                if not answered and expected.encode() in output:
                    os.write(descriptor, answer.encode() + b'\n')
                    answered = True
            child, status = os.waitpid(pid, os.WNOHANG)
            if child:
                completed = True
                assert answered and os.waitstatus_to_exitcode(status) == 0, output.decode(errors='replace')
                return
        raise AssertionError('Prompt timed out: ' + output.decode(errors='replace'))
    finally:
        if not completed:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        os.close(descriptor)

with tempfile.TemporaryDirectory(prefix='forklaunch-name-prompts-') as directory:
    app = Path(directory) / 'app'
    terminal(['init', 'application', '--language', 'rust', '--path', str(app), '-D', 'Prompt fixture', '-A', 'ForkLaunch', '-L', 'MIT'],
             'Rust application name', 'prompt-app')
    assert tomllib.loads((app / '.forklaunch/manifest.toml').read_text())['app_name'] == 'prompt-app'
    for kind, answer, name in [('service', 'accounts', 'accounts'), ('library', '', 'my-library'), ('worker', 'jobs', 'jobs')]:
        terminal(['init', kind, '--language', 'rust', '--path', str(app), '-D', 'Prompt fixture'],
                 f'Rust {kind} crate name', answer)
        # Current layout: src/<name>, libraries under src/core/<name>.
        manifest_path = next(path for path in [app / 'src' / name / 'Cargo.toml', app / 'src/core' / name / 'Cargo.toml']
                             if path.is_file())
        cargo = tomllib.loads(manifest_path.read_text())
        assert cargo['package']['name'] == name
    manifest = (app / '.forklaunch/manifest.toml').read_bytes()
    for name in [None, 'bad name']:
        args = [str(binary), 'init', 'service', '--language', 'rust', '--path', str(app)]
        if name is not None:
            args.append(name)
        result = subprocess.run(args, env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=10)
        assert result.returncode != 0, result.stdout
        assert ('Missing service name' if name is None else 'name cannot') in result.stdout + result.stderr
        assert (app / '.forklaunch/manifest.toml').read_bytes() == manifest
print('PASS native application/service/library/worker terminal naming, default selection, invalid-name rejection and noninteractive no-hang/no-write behavior; temporary workspace removed')
