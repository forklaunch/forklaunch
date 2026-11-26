"""Deterministic temporary directory names for CI test scripts.

Loaded through PYTHONPATH by ./.github/actions/rust-build-cache when
`stable-tmp` is on. sccache keys include the working directory, and test
scripts scaffold their applications under tempfile.mkdtemp() directories,
so random names meant the embedded framework crates never matched between
runs. Names become <prefix>fl<NNNN><suffix>, counting up from 0000 and
skipping any that exist, so a script that runs the same steps gets the same
paths every run. Each call still creates a new, empty, private directory.
"""
import os
import tempfile

if os.environ.get('FORKLAUNCH_STABLE_TMP') == '1':
    _original_mkdtemp = tempfile.mkdtemp
    _next = {}

    def _stable_mkdtemp(suffix=None, prefix=None, dir=None):
        if any(isinstance(part, bytes) for part in (suffix, prefix, dir)):
            return _original_mkdtemp(suffix, prefix, dir)
        prefix = tempfile.template if prefix is None else prefix
        suffix = '' if suffix is None else suffix
        base = tempfile.gettempdir() if dir is None else os.fspath(dir)
        key = (base, prefix, suffix)
        number = _next.get(key, 0)
        while True:
            path = os.path.join(base, f'{prefix}fl{number:04d}{suffix}')
            number += 1
            try:
                os.mkdir(path, 0o700)
            except FileExistsError:
                continue
            _next[key] = number
            return os.path.abspath(path)

    tempfile.mkdtemp = _stable_mkdtemp
