"""Exercise scaffold ignore bytes with real Git, without building/running a CLI."""
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCES = ['cli/src/templates/application/.gitignore', 'blueprint/interfaces/iam/.gitignore', 'blueprint/interfaces/billing/.gitignore']

class ScaffoldIgnoreTests(unittest.TestCase):
    def test_generator_uses_the_tested_template(self):
        source = (ROOT / 'cli/src/core/gitignore.rs').read_text()
        self.assertIn('include_str!("../templates/application/.gitignore").to_owned()', source)
        self.assertIn('if path.exists()', source)  # Existing customer ignores aren't overwritten.

    def test_real_git_never_stages_private_environment_files(self):
        for template in SOURCES:
            with self.subTest(template=template), tempfile.TemporaryDirectory(prefix='fl-ignore-proof-') as td:
                root = pathlib.Path(td)
                env = {'PATH': os.environ['PATH'], 'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null'}
                def git(*args):
                    return subprocess.run(['git','-c','core.hooksPath=/dev/null',*args],cwd=root,env=env,capture_output=True,text=True,timeout=10,check=True).stdout
                git('init','--quiet')
                (root / '.gitignore').write_bytes((ROOT / template).read_bytes())
                private=[]; public=[]
                for folder in ['', 'src/modules/iam', 'src/modules/records', 'apps/client']:
                    base=root / folder; base.mkdir(parents=True,exist_ok=True)
                    for name in ['.env','.env.local','.env.test','.env.production','.env.production.local','.env.development','.env.backup']:
                        file=base/name;file.write_text('SYNTHETIC_PRIVATE_VALUE=fixture-only\n');private.append(file.relative_to(root).as_posix())
                    for name in ['.env.example','.env.sample','.env.template']:
                        file=base/name;file.write_text('PUBLIC_PLACEHOLDER=\n');public.append(file.relative_to(root).as_posix())
                for file in private:
                    self.assertEqual(git('check-ignore','--no-index',file).strip(),file)
                git('add','--all')
                staged=set(git('ls-files').splitlines())
                self.assertTrue(set(private).isdisjoint(staged))
                self.assertTrue(set(public).issubset(staged))

if __name__ == '__main__':
    unittest.main()
