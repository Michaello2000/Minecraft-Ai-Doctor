# Validation record — October 25, 2026

## Project rename: Minecraft Ai Doctor

The app, Cargo package, executable, CLI/report headings and current setup instructions now use **Minecraft Ai Doctor** (`minecraft-ai-doctor.exe`). Historical entries below retain previous artifact names. The legacy Credential Manager service and local data directory are intentionally unchanged so saved keys, cached results and repair backups remain accessible.

The renamed source passed `cargo fmt --all -- --check`, all-target Clippy with `-D warnings`, `cargo test --offline --locked` (**42 passed, 2 opt-in network tests ignored**) and `cargo build --release --offline --locked`. The publication checker found no secrets in the renamed **60-file candidate** and passed 14 sensitive-path ignore probes. No Git repository, commit, upload or submission was created. The local post draft is `DEV_POST.md`; demo and public source links, plus live Gemma validation, are still pending.

## Current update: dependency parsing and artwork

This update supersedes the executable/test counts in the historical milestones below. The original `dist/prism-ai-doctor.exe` was in use, so the new verified binary is **`dist/prism-ai-doctor-updated.exe`**. The running app was not stopped or overwritten. The new file matches the normal release build by SHA-256; `dist/SHA256SUMS.txt` names this artifact.

Formatting, all-target Clippy with `-D warnings`, and the normal Windows release build passed. `cargo test --offline --locked` passed **42 tests** (34 unit + 8 integration), with **2 opt-in network tests ignored**. The approved `live_nvidium_artwork` test was run separately and passed: Modrinth's Nvidium project by cortex provided a **5,296-byte WebP**, which decoded to 64 × 64 and reloaded through the cached project binding. No mod binary, log or API key was transferred in this check.

A new synthetic fixture first reproduced the exact parsing failure: Fabric's `of mod 'Sodium'` became ID `mod`, and `but only the wrong versions are present` became installed version `the`. The regression now verifies both mod identities, the actual Sodium versions, the Nvidium replacement target and its icon identity. The final executable also imported the actual Lunar profile read-only and reported Sodium 0.5.9 / 0.5.11 required by Nvidium, versus 0.8.12-beta.2+mc1.21.1 and 0.6.9+mc1.21.1 found.

Profile artwork tests check Prism `iconKey`, configured names, Dawn names and rejection of path escapes. The live library shows locally exposed Prism artwork (including the Adrenaline WebP), and normal discovery reads Dawn's friendly names. Icons embedded only in a launcher's executable are not extracted; those profiles use a readable fallback. Named local profile files are supported for Lunar/custom layouts, but no local Lunar icon was exposed by the inspected profile.

Renderer captures were inspected for the two-step replacement panel with the real downloaded Nvidium icon and for the instance list with local artwork. The packaged normal executable then rendered startup discovery and the Sodium-conflict fixture and exited normally (three-frame smoke checks). Full manual mouse/file-picker testing of the updated lookup flow is not claimed. No personal instance was modified and no game was launched.

## Latest revision: SVG, grouped diagnosis, repair copies and caching

The sections below retain earlier milestones. This section describes the current executable and supersedes earlier statements that there is no repair-copy/download capability or persistent API-result cache.

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --offline --locked --all-targets -- -D warnings`: passed. The restricted filesystem emitted incremental-cache hard-link fallback notices; no Clippy diagnostic remained.
- `cargo test --offline --locked`: **39 passed, 0 failed, 1 ignored** (31 unit tests passed, 8 integration tests passed). The ignored test is the separately opt-in live Modrinth metadata test. Local tests ran with normal Windows file-rename and loopback-socket access; the sandbox denies those operations, so its initial restricted run failed for that environmental reason.
- `cargo build --release --offline --locked`: passed with the normal feature set. The packaged executable matches the release binary by SHA-256; see `dist/SHA256SUMS.txt`.
- The packaged executable rendered three frames and closed with exit 0 for startup discovery and for the detailed dependency/version fixture.

Repair tests construct disposable metadata-only JARs, apply duplicate removal or a SHA-512-verified local replacement to a separate copy, and restore it. They check unchanged source files, changed-source refusal both before plan and before apply, changed-target refusal, path escape rejection, mod identity/hash rejection, restart recovery and abandoned temporary journals. The download stream uses a loopback fixture to check transient retries, size rejection and cancellation. No real mod binaries were downloaded, no personal instance was repaired, and no Minecraft process was launched. A complete runnable launcher clone is not implemented: repair copies contain top-level mods and backups only.

AI tests prove a seeded valid cached response returns without credentials/network and that provider/model/payload changes produce a different key. Unsupported AI repair indexes are rejected. Local report cache invalidation, stale/corrupt disk cache handling, scoped cache clearing, local PNG icon loading and unsafe icon fallback are covered. Raw logs and API keys are excluded from persistent caches; redaction remains best effort. Public metadata caches last 6 hours and AI responses 7 days.

Renderer captures were inspected at 1280 × 880 (crowded duplicate fixture and repair review) and 900 × 750 (detailed version/dependency fixture). These use the temporary eframe screenshot feature; it is absent from the final normal build. The SVG source is preserved unchanged at `assets/logo.svg`; its Figma angular gradient is normalized for native SVG rendering and Windows icon generation. The final executable icon was extracted and inspected. Native mouse/file-picker/clipboard interactions for the newly added repair controls have not been manually exercised; automated worker/backend and egui rendering tests cover their underlying paths.

The final normal executable also imported a selected real Prism instance read-only: **1 log, 118 mods, 212 total findings, 4 warnings**. A Lunar profile imported **2 logs, 0 top-level mods, 1 finding, 1 warning**. These totals include unconfirmed metadata observations and are not counts of proven broken mods. Logs were analyzed locally and were not copied into the repository or sent to a provider. Live Google/Groq inference, a real CDN mod transfer and a successful Minecraft launch remain unverified.

## Environment

Work ran against local Windows paths with sandbox restrictions, with approved access for Cargo downloads/toolchain operations and local mock sockets. Rust `1.98.1`, Cargo `1.98.1`, Git `2.55.0.windows.3`, stable `x86_64-pc-windows-msvc`, Visual Studio 18 Build Tools and Windows SDK `10.0.26100.0` were found. PowerShell was `7.6.5`. No machine-wide installation or configuration change was needed.

The provided unsigned `scripts/check-windows.ps1` was blocked by Windows execution policy. Its equivalent tool/version, MSVC, SDK and launcher-directory checks were run manually after reviewing the script. Execution policy was not changed. The original starter documents and script were preserved. The app inventory independently located Prism's executable under the real user's LocalAppData; the shell sandbox's redirected LocalAppData did not resolve that installation.

Rust dependency downloads from crates.io and limited GUI tests were explicitly approved. No Minecraft/modpack downloads or authenticated cloud requests were performed.

## Initial automated checks

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --offline --all-targets -- -D warnings`: passed.
- `cargo test --offline`: **18 passed, 0 failed, 0 ignored** (11 unit tests and 7 integration tests). Cargo regenerated the lockfile after an unused HTTP feature was removed; subsequent build/lint commands used that lockfile.
- `cargo build --release --locked --offline`: passed for Windows x64/MSVC.
- Packaged executable: GUI smoke with the missing-dependency fixture, and GUI smoke with startup discovery. Each rendered three frames and exited with code 0.

Coverage includes the six requested log fixtures, exact log and descriptor line references, malformed UTF-8, bounded logs, malformed/oversized/high-ratio ZIP metadata, archive traversal, Fabric/Quilt/Forge descriptors, duplicate IDs, conservative version matching, loader mismatch evidence, Prism metadata, hook behavior on successful exits, CLI errors/import, safe create-new exports, protected-folder rejection, redaction (including Authorization/Bearer headers), repeatable Lab hashes and symlink escape rejection. The Windows symlink fixture was successfully exercised in the targeted diagnostic run; no privilege fallback message appeared.

Provider tests use local loopback mock HTTP servers and synthetic credentials only. They exercise both adapters' JSON envelopes, authentication headers, body/preview equality, 429/503 recovery, 401/403/404/429/500 failures, timeouts, cancellation, malformed JSON, invented citations, citations attached to the wrong finding and invented structured mod/version claims. These tests do **not** prove either live provider accepted a real account/key.

## Read-only local file checks

Standard Prism instances and Lunar profiles were found. A Prism `1.21.1` instance imported **1 log and 121 top-level Fabric descriptors**, producing **229 findings, 4 warnings and 6 duplicate-ID findings**. This is a mixture of log signals, metadata observations and unknowns, not 229 proven runtime failures. The duplicate findings reflect parsed IDs in separate top-level descriptors; nested resolution and complete loader behavior are not established.

A Lunar `1.21` profile imported **2 logs, 0 exposed top-level mod descriptors**, producing **2 findings and 1 warning**. Manual import of its `latest.log` alone returned no recognized crash signal. No real instance, mod, world, account file or launcher configuration was modified by analysis. Personal logs were not copied into the repository or uploaded.

No Fabulously Optimized installation or clean runtime baseline was identified and tested as such. No game was launched and no real mod compatibility outcome is claimed.

## GUI observations

Computer Use launched the release app, observed standard profile discovery, navigated to Analyze, entered the synthetic missing-dependency fixture, ran analysis and expanded original evidence lines. That interactive pass used an earlier release build; later evidence/privacy/style corrections were covered by tests and final executable rendering smoke checks.

Prism Launcher `11.1.0` opened to a pre-existing PolyMC migration reminder. No response was submitted, no settings changed and no game launched. The reminder was left for the user. A Lunar launcher executable/window was not found in the available app inventory; its local logs were tested instead.

Later Computer Use snapshots returned an unrelated foreground app, so GUI input was stopped. Only the Doctor test process created for this work was stopped to permit rebuilding. The final smoke processes exited normally. No launcher/game process was terminated.

## Remaining verification gaps

- Live Google/Groq credentials, account quota and model access; actual model output quality. Configure a key in Settings and review/send a synthetic payload manually.
- Credential Manager write/delete interaction with a real key. No user credentials were created, read from chat or changed during development.
- Full interactive file-picker/export flow, all settings interactions, clipboard contents and resized-window QA on the final build. The corresponding file export logic is covered by automated tests; the app's final window rendering is smoke-tested.
- Actual post-exit command execution inside Prism, Lunar's launcher GUI, game launches and real modpack experiments.
- Code signing, an installer, non-Windows builds and a public release. The executable remains a portable unsigned local build.
- No Git repository, commits, pushes, release publication, video recording or DEV submission was created. Submission notes contain a draft and required next steps.

The final executable and SHA-256 are recorded in `dist/SHA256SUMS.txt`. Source and dependency versions are in this folder and `Cargo.lock`.

## Results-screen correction — October 25, 2026

The reported broken expanders were reproduced in a failing egui regression test: two `Evidence & context` headers in separate groups received the same persistent ID. Evidence headers now use explicit per-finding IDs. The duplicate-label case and actual simulated pointer press/release sequences verify that opening/closing one header leaves another independent. Mod descriptor headers also receive explicit IDs.

The results screen now uses a compact issue selector and one visible fix checklist. Other observations, source evidence, import notes and optional cloud analysis use progressive disclosure. Copy/export actions are grouped in the Report menu. No finding or privacy approval flow was removed.

Evidence-derived fix guides name missing dependency IDs/requirements, report the observed Java class version, preserve the installed game baseline when suggesting a replacement mod build, and list duplicate JAR filenames when metadata is available. Uncertain causes stay qualified. Copy fix steps and Open mods folder assist manual review; no automatic mod changes, downloads or runtime setting changes were introduced. Markdown reports include these checklists.

The expanded suite passes **24 tests, 0 failures, 0 ignored** (17 unit and 7 integration tests), including the click-interaction regression and dependency/Java/version/uncertain-remedy cases. Formatting, warnings-as-errors Clippy and the normal release build pass. The packaged normal executable renders the two-issue fixture in GUI smoke mode.

Native desktop screenshots again showed an unrelated foreground app, so no input was sent to it. For visual inspection, a temporary build used eframe's built-in `__screenshot` test feature and captured its own renderer with the synthetic two-issue fixture. The results/checklist layout was inspected at the default 1120 × 790 content size. The packaged executable uses the normal feature set; screenshot test mode is not shipped. Smoke inputs are preloaded before rendering to avoid capturing the background-import loading state. Final native pointer interaction and resized-window QA remain unverified; click behavior is covered by the egui input tests.

## Launcher groups, pasted console, release lookup and branding

The user approved the dark design direction and public Modrinth metadata verification. Global Impeccable and Design System guidance informed the quieter navigation, shared theme tokens, aligned profile rows, launcher tabs, compact issue selector, responsive fix/version layout and supplied ring branding. The original PNG was copied unchanged to `assets/chroma-ring.png`; its ICO conversion is embedded into the Windows executable through the SDK resource compiler. The same image is used for the native window icon and interface logo.

Profile discovery found **12 Prism, 4 Lunar and 7 Dawn directories** during the rendering check. Dawn's `%APPDATA%\.dawn\profiles` root was verified locally; this remains a layout heuristic with manual import fallback. Discovery enumerates profiles without opening account data. No instance or game was modified or launched.

Paste-console analysis runs through a background worker without writing a temporary file. Tests cover blank and oversized input, original line numbers, a complete UI-worker import, rendering at two content widths, distinct dependency errors and removal of a redundant generic resolver error. Runtime inference uses the finding's own log rather than an unrelated/stale imported log. Required dependencies and version errors remain separate actionable findings.

Normal `cargo test --offline --locked` now passes **31 tests** (23 unit and 8 integration), with **one network test explicitly ignored by default**. That network test was separately run with approval using `cargo test --offline --locked live_public_metadata_lookup -- --ignored --nocapture`: it passed, searched the public Nvidium project by cortex and parsed **release 0.3.1 for Minecraft 1.21.1 / Fabric**. No logs, credentials or binary mod files were transferred. The test does not prove runtime compatibility with any real modpack.

Additional tests validate project/game/loader/range filtering, rejection of invalid responses/IDs, result isolation when changing findings and the supplied logo/icon data. Version lookup uses public metadata with explicit per-action requests. Search results require project/author confirmation; releases are not treated as proof of compatibility with all other mods. Unknown runtime or unsupported version syntax does not produce a guessed replacement.

Formatting, warnings-as-errors Clippy and the normal Windows release build pass. Renderer captures were inspected at **1280 × 880** and **900 × 750**, plus the launcher-library screen. A column-width issue and uneven row actions found during inspection were corrected and confirmed in the second capture pass. Narrow windows expose version guidance through progressive disclosure instead of overlapping the fix text. The captures used the temporary eframe screenshot feature; the shipped build uses the normal feature set.

Remaining gaps: native mouse/clipboard interaction with the new paste/lookup controls, a real account-bound Minecraft run and live cloud inference are not claimed. The paste workflow and egui input behavior are covered by automated tests, and the Modrinth client was exercised against the real public API. Online release results are session data and copied separately; current report export remains the local diagnosis. The app performs no automatic mod changes or downloads.
