"""Local publication preflight. Requires Python 3.10+, Git, and Gitleaks 8.

Uses a disposable Git index to apply ignore rules without changing this project.
No credentials, network requests, commits, remotes, or pushes are used.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PRIVATE_DIRS = {
    ".git", ".tools", ".publish-review", "target", "dist", "lab-workspace",
    ".cargo-cache", "output", "exports", "reports", "screenshots",
    "cache-v1", "repair-copies-v1",
}
PRIVATE_SUFFIXES = {
    ".exe", ".dll", ".pdb", ".pem", ".key", ".p8", ".p12", ".pfx", ".der",
    ".kdbx", ".sqlite", ".sqlite3", ".db", ".bak", ".backup", ".pending",
    ".tmp", ".jar", ".mrpack", ".zip", ".mp4", ".dmp", ".hprof", ".jfr",
}


def private_path(name):
    path = Path(name)
    parts = [p.casefold() for p in path.parts]
    base = parts[-1]
    if name == ".env.example":
        return False
    return (
        any(p in PRIVATE_DIRS for p in parts)
        or path.suffix.casefold() in PRIVATE_SUFFIXES
        or base.startswith((".env", "credentials.", "secrets.", "api-key", "api_keys", "apikey"))
        or base in {".netrc", ".npmrc", "keys.txt", "auth.json", "launcher_accounts.json"}
        or (base.startswith("accounts") and base.endswith(".json"))
        or base.endswith(".local.json")
        or (base.endswith(".log") and not name.startswith("tests/fixtures/"))
    )


def environment():
    # Do not pass provider keys or scanner/config overrides to subprocesses.
    allowed = {"PATH", "SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP"}
    env = {k: v for k, v in os.environ.items() if k.upper() in allowed}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull, GIT_TERMINAL_PROMPT="0")
    return env


def check_tree(root):
    count = 0
    for directory, folders, files in os.walk(root, followlinks=False):
        folders[:] = [f for f in folders if f.casefold() not in PRIVATE_DIRS and f != "__pycache__"]
        for name in folders + files:
            path = Path(directory) / name
            attributes = getattr(path.lstat(), "st_file_attributes", 0)
            if path.is_symlink() or attributes & 0x400:
                raise RuntimeError("Links/reparse points in the source tree require manual review")
            count += 1
            if count > 10000:
                raise RuntimeError("Source tree exceeds the 10,000-entry review limit")


def run(args, cwd, env, data=None, check=True):
    result = subprocess.run(args, cwd=cwd, env=env, input=data, capture_output=True, timeout=120)
    if check and result.returncode:
        raise RuntimeError(f"{Path(args[0]).name} operation failed (exit {result.returncode}); no file contents printed")
    return result


def snapshot(git, repo, worktree, destination, env):
    command = [git, f"--git-dir={repo}", f"--work-tree={worktree}"]
    entries = run(command + ["ls-files", "--stage", "-z"], worktree, env).stdout.split(b"\0")
    names = []
    for entry in filter(None, entries):
        header, name = entry.split(b"\t", 1)
        mode, _, stage = header.split()
        name = name.decode("utf-8")
        if mode not in {b"100644", b"100755"} or stage != b"0":
            raise RuntimeError("Links, submodules or unresolved merge entries require manual review")
        if private_path(name):
            raise RuntimeError(f"Private/artifact path in upload selection: {name}")
        names.append(name)
    if not names:
        raise RuntimeError("Upload selection is empty")
    destination.mkdir()
    run(command + ["checkout-index", "--all", f"--prefix={destination.as_posix()}/"], worktree, env)
    return names


def scan(scanner, kind, path, scratch, env):
    # Always use the built-in rules, without project-specific suppressions.
    config = scratch / "scanner.toml"
    config.write_text("[extend]\nuseDefault = true\n", encoding="utf-8")
    report = scratch / f"{kind}-result.json"
    args = [scanner, "git" if kind == "history" else "dir", str(path),
            "--config", str(config), "--redact=100", "--no-banner", "--ignore-gitleaks-allow",
            "--report-format=json", "--report-path", str(report)]
    if kind == "history":
        args.append("--log-opts=--all")
    result = run(args, scratch, env, check=False)
    if result.returncode:
        if result.returncode == 1 and report.exists():
            findings = json.loads(report.read_text(encoding="utf-8")) or []
            for item in findings:
                # Never print Match, Secret, source snippets, or scanner stderr.
                print(f"BLOCKED: {Path(item['File']).name}:{item['StartLine']} [{item['RuleID']}]")
        raise RuntimeError(f"{kind} secret scan failed (exit {result.returncode}); publication blocked")
    print(f"{kind}: Gitleaks found no secrets")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scanner", help="Path to the official verified Gitleaks executable")
    parser.add_argument("--list-only", action="store_true", help="Check file selection only; does NOT scan secrets")
    args = parser.parse_args()
    git = shutil.which("git")
    if not git:
        raise RuntimeError("Git is required")
    env = environment()
    check_tree(ROOT)
    scratch_parent = ROOT / ".publish-review"
    scratch_parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="check-", dir=scratch_parent) as temporary:
        scratch = Path(temporary)
        candidate_repo = scratch / "candidate.git"
        run([git, "init", "--bare", str(candidate_repo)], ROOT, env)
        command = [git, f"--git-dir={candidate_repo}", f"--work-tree={ROOT}"]
        run(command + ["add", "--all"], ROOT, env)
        # Exact NUL-delimited ignore probes, including nested credentials.
        probes = [".env", "nested/.env.production", "dist/app.exe", "target/personal.log",
                  "exports/report.json", "secrets.json", "auth.json", "certificate.pem",
                  "backup.zip", "nested/accounts.json", "cache-v1/ai.json", "repair-copies-v1/mod.jar",
                  ".tools/gitleaks/gitleaks.exe", ".publish-review/private.json"]
        payload = b"\0".join(p.encode() for p in probes) + b"\0"
        ignored = run(command + ["check-ignore", "--no-index", "--stdin", "-z"], ROOT, env, payload).stdout
        if set(ignored.rstrip(b"\0").split(b"\0")) != {p.encode() for p in probes}:
            raise RuntimeError("Sensitive-path ignore probes failed")
        candidate = scratch / "candidate"
        names = snapshot(git, candidate_repo, ROOT, candidate, env)
        print(f"Candidate: {len(names)} files; {len(probes)} sensitive-path ignore probes passed")
        for name in names:
            print(f"  {name}")
        real = run([git, "rev-parse", "--absolute-git-dir"], ROOT, env, check=False)
        staged = None
        if real.returncode == 0:
            staged = scratch / "staged"
            snapshot(git, Path(real.stdout.decode().strip()), ROOT, staged, env)
        if args.list_only:
            print("File-selection checks only. NO secret scan performed.")
            return
        scanner = args.scanner or shutil.which("gitleaks")
        if not scanner:
            local = ROOT / ".tools" / "gitleaks" / "gitleaks.exe"
            if local.is_file():
                scanner = str(local)
        if not scanner:
            raise RuntimeError("Gitleaks is required for a full preflight; use --scanner <verified executable>")
        scan(scanner, "candidate", candidate, scratch, env)
        if staged:
            scan(scanner, "staged", staged, scratch, env)
            if run([git, "rev-parse", "--verify", "HEAD"], ROOT, env, check=False).returncode == 0:
                scan(scanner, "history", ROOT, scratch, env)
            else:
                print("History: no commits yet")
        else:
            print("Index/history: no project Git repository exists; only the exact proposed upload was scanned")
        print("Preflight passed for this snapshot. Re-run after staging and before every push. Nothing was committed or uploaded.")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"Preflight stopped: {error}", file=sys.stderr)
        sys.exit(1)
