#!/usr/bin/env python3
"""Serialize fixture workspaces that share Cargo's jobs executable output path."""
import fcntl
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile

# Each suite builds a package named jobs. A shared target cache is useful but
# concurrent suites must not execute another backend's overwritten binary.
target = os.environ.get("CARGO_TARGET_DIR", "forklaunch-worker-runtime")
key = hashlib.sha256(target.encode()).hexdigest()[:16]
path = Path(tempfile.gettempdir()) / f"forklaunch-worker-runtime-{os.getuid()}-{key}.lock"
with path.open("a") as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    result = subprocess.run(["bash", *sys.argv[1:]], env={**os.environ, "FORKLAUNCH_WORKER_RUNTIME_LOCKED": "1"})
    sys.exit(result.returncode)
