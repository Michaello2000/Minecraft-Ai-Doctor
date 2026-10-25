"""Tests use disposable repositories; the project's Git state is never changed."""
import importlib.util
from pathlib import Path
import shutil
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("check_publish", Path(__file__).parents[1] / "check-publish.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class PublicationChecks(unittest.TestCase):
    def test_private_paths_and_synthetic_fixtures(self):
        for name in [".env", "nested/.env.production", "dist/app.EXE", "private.pem",
                     "nested/accounts.json", "exports/report.json", "x.local.json"]:
            self.assertTrue(CHECK.private_path(name), name)
        for name in [".env.example", "tests/fixtures/missing.log", "src/providers.rs", "Cargo.lock"]:
            self.assertFalse(CHECK.private_path(name), name)

    def test_index_snapshot_uses_staged_bytes_and_rejects_forced_private_files(self):
        git = shutil.which("git")
        self.assertIsNotNone(git)
        scratch = CHECK.ROOT / ".publish-review"
        scratch.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            env = CHECK.environment()
            CHECK.run([git, "init", str(source)], root, env)
            document = source / "README.md"
            document.write_text("reviewed staged text", encoding="utf-8")
            CHECK.run([git, "add", "README.md"], source, env)
            document.write_text("changed working text", encoding="utf-8")
            staged = root / "snapshot"
            CHECK.snapshot(git, source / ".git", source, staged, env)
            self.assertEqual((staged / "README.md").read_text(), "reviewed staged text")
            (source / ".env").write_text("synthetic fixture only", encoding="utf-8")
            CHECK.run([git, "add", "--force", ".env"], source, env)
            with self.assertRaisesRegex(RuntimeError, "Private/artifact"):
                CHECK.snapshot(git, source / ".git", source, root / "rejected", env)

    def test_subprocess_environment_does_not_contain_provider_credentials(self):
        env = CHECK.environment()
        self.assertNotIn("GEMINI_API_KEY", env)
        self.assertNotIn("GROQ_API_KEY", env)
        self.assertNotIn("GITLEAKS_CONFIG", env)


if __name__ == "__main__":
    unittest.main()
