"""Copy the universal-sdk sources as an installable package.

Its package.json names sibling packages as `workspace:` ranges, which npm
cannot install outside the pnpm workspace; publishing rewrites them to the
siblings' versions, so the copy does the same. Only the devDependencies the
build uses are kept; the package's own test tooling is not installed.
"""
import json
from pathlib import Path
import shutil

BUILD_TOOLS = {'@types/node', 'openapi3-ts', 'tsup', 'typescript'}


def copy_universal_sdk(repo, destination):
    framework = Path(repo) / 'framework'
    shutil.copytree(framework / 'universal-sdk', destination,
                    ignore=shutil.ignore_patterns('node_modules', 'lib', 'docs', '__test__', 'vitest.config.ts'))
    # The package's tsconfig extends the framework's shared base config.
    shutil.copy2(framework / 'tsconfig.base.json', Path(destination) / 'tsconfig.base.json')
    tsconfig = Path(destination) / 'tsconfig.json'
    tsconfig.write_text(tsconfig.read_text().replace('"../tsconfig.base.json"', '"./tsconfig.base.json"'))
    versions = {}
    for manifest in framework.glob('*/package.json'):
        package = json.loads(manifest.read_text())
        if 'name' in package and 'version' in package:
            versions[package['name']] = package['version']
    path = Path(destination) / 'package.json'
    package = json.loads(path.read_text())
    package['devDependencies'] = {name: spec for name, spec in package.get('devDependencies', {}).items() if name in BUILD_TOOLS}
    # The shared base config loads vitest's global types.
    package['devDependencies']['vitest'] = json.loads((framework / 'package.json').read_text())['devDependencies']['vitest']
    for section in ('dependencies', 'peerDependencies', 'optionalDependencies', 'devDependencies'):
        for name, spec in package.get(section, {}).items():
            if spec.startswith('workspace:'):
                range_ = spec.removeprefix('workspace:')
                version = versions[name]
                package[section][name] = version if range_ in ('', '*') else (range_ if range_ not in ('^', '~') else range_ + version)
    path.write_text(json.dumps(package, indent=2) + '\n')
