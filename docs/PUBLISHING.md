# Preparing a GitHub upload

Publish source files, reviewed documentation, synthetic fixtures and the supplied logo assets. Keep API keys in the app's Windows Credential Manager entries or process environment. Never fill `.env.example` with a real key. The application does not need a bundled key, `.env` file, personal log or prebuilt executable.

The ignore rules exclude build output, local tools, exports, reports, screenshots, runtime caches, repair copies, credentials, backups, starter inputs and mod archives. Only the reviewed root `.env.example` and deliberately synthetic `tests/fixtures/*.log` files are exceptions. Ignore rules do not remove a file already committed or force-added.

## Local preflight

The optional publication checker requires Python 3.10+, Git and an official [Gitleaks release](https://github.com/gitleaks/gitleaks/releases). These are development tools, not application runtime dependencies. Verify the scanner download against the release's published checksums. A portable Windows copy can be placed at `.tools/gitleaks/gitleaks.exe`; this directory is ignored. No machine-wide installation is needed.

Run in the project root:

```powershell
python scripts/check-publish.py
```

Or provide an installed scanner:

```powershell
python scripts/check-publish.py --scanner 'D:\Tools\gitleaks.exe'
```

The checker:

- Builds an exact candidate snapshot through a disposable Git index using the current ignore rules. It does not initialize this project, stage its files, change remotes or upload anything.
- Checks sensitive-path ignores with NUL-delimited probes and refuses links/reparse points, private/artifact paths, submodules and merge conflicts in the upload selection.
- Runs Gitleaks locally with redacted output and its default rules. Inline `gitleaks:allow` comments cannot suppress findings. It does not inherit provider keys or scanner-configuration overrides. Detected values and source snippets are never printed.
- If this project has Git metadata, separately scans the actual staged bytes and all history reachable through `--all`. Working files and staged files can contain different data.
- Deletes temporary source/index snapshots after the check. It fails closed when a scanner is unavailable or a scan fails.

`--list-only` checks the proposed file selection and ignores without scanning for secrets. It is useful for reviewing filenames, but is **not** clearance to push. Scanner results cover that snapshot only; rerun after any change or staging operation. Scanners do not prove that arbitrary personal text is safe to publish, so manually inspect new fixtures and documentation too.

To run the checker tests:

```powershell
python -m unittest discover -s scripts/tests -v
```

## Before the first push

This workspace initially has no Git repository, index or history. Preparing files does not publish them. Once you choose to initialize Git and stage the reviewed source, rerun the full preflight and inspect the staged diff before committing. Do not force-add ignored files to bypass the checker.

Create the GitHub repository with the intended visibility and enable the available [secret scanning and push protection](https://docs.github.com/en/code-security/concepts/secret-security/push-protection). Push protection is an additional safeguard; it does not replace the local check. Never bypass a detected real key. If a genuine key has already entered a commit, revoke/rotate it and review the history before publishing; deleting only its working file is insufficient.

No release, binary upload or GitHub Actions secret is required for this application's users to configure their own API credentials. Keep local EXEs and QA screenshots out of the initial source upload. Review binaries and screenshots separately before any future release.

## Preparation verification — October 25, 2026

The official Gitleaks 8.30.1 Windows x64 archive was downloaded to the ignored `.tools/gitleaks` folder after approval. Its SHA-256 matched the release's published checksum: `d29144deff3a68aa93ced33dddf84b7fdc26070add4aa0f4513094c8332afc4e`.

The exact **59-file candidate** passed the full local Gitleaks scan with no detected secrets, and **14 sensitive-path ignore probes** passed. The three publication-check tests passed, including staged-byte isolation and rejection of force-added private files. A separate disposable test proved that a synthetic Google-format key is blocked even with an inline allow comment, without printing the value. The synthetic file and temporary index/snapshot files were removed after testing.

There is no project Git repository, index or history yet, so no existing project history was scanned. No repository, commit, remote, push or upload was created. These results apply to the reviewed source snapshot only. Rerun the preflight after initializing/staging Git and before pushing; new commits or files require another check. A clean scan does not guarantee that every possible secret or private sentence can be recognized.
