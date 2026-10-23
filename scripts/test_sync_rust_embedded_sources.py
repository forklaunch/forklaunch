"""Embedded ignore-file safety regressions; no repository files are mutated."""
import tempfile
import unittest
from pathlib import Path
from sync_rust_embedded_sources import FRAMEWORK_SOURCES, ROOT, mirror_framework_source, mirror_git_metadata


class CatalogTests(unittest.TestCase):
    def test_git_metadata_is_regular_owned_copy_and_updates(self):
        for name in ['.gitignore', '.gitattributes']:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source, catalog = root / 'source', root / 'catalog'
                source.mkdir()
                catalog.mkdir()
                original, embedded = source / name, catalog / name
                original.write_text('original\n')
                embedded.symlink_to(original)
                mirror_git_metadata(original, embedded)
                self.assertFalse(embedded.is_symlink())
                self.assertEqual(embedded.read_bytes(), original.read_bytes())
                modified = embedded.stat().st_mtime_ns
                mirror_git_metadata(original, embedded)
                self.assertEqual(embedded.stat().st_mtime_ns, modified)
                original.write_text('updated\n')
                mirror_git_metadata(original, embedded)
                self.assertEqual(embedded.read_bytes(), original.read_bytes())

    def test_unexpected_git_metadata_link_is_not_replaced(self):
        for name in ['.gitignore', '.gitattributes']:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source, catalog = root / 'source', root / 'catalog'
                source.mkdir()
                catalog.mkdir()
                (source / name).write_text('original\n')
                other = root / 'user-owned'
                other.write_text('preserve\n')
                embedded = catalog / name
                embedded.symlink_to(other)
                with self.assertRaises(ValueError):
                    mirror_git_metadata(source / name, embedded)
                self.assertTrue(embedded.is_symlink())
                self.assertEqual(embedded.resolve(), other.resolve())
                self.assertEqual(other.read_text(), 'preserve\n')


    def test_framework_sources_are_regular_current_copies(self):
        # Containers and registry packages see only framework-rs; a link out of it breaks include_str!.
        for source, target in FRAMEWORK_SOURCES:
            with self.subTest(target=target):
                self.assertFalse((ROOT / target).is_symlink(), f'{target} must be a regular file')
                self.assertEqual((ROOT / target).read_bytes(), (ROOT / source).read_bytes(),
                                 'run scripts/sync_rust_embedded_sources.py')

    def test_framework_source_link_becomes_copy_and_check_reports_stale(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, target = root / 'owner.ts', root / 'crate/src/copy.ts'
            source.write_text('export const a = 1;\n')
            target.parent.mkdir(parents=True)
            target.symlink_to(source)
            self.assertFalse(mirror_framework_source(source, target, check=True))
            self.assertTrue(target.is_symlink())
            self.assertTrue(mirror_framework_source(source, target))
            self.assertFalse(target.is_symlink())
            self.assertEqual(target.read_bytes(), source.read_bytes())
            source.write_text('export const a = 2;\n')
            self.assertFalse(mirror_framework_source(source, target, check=True))


if __name__ == '__main__':
    unittest.main()
