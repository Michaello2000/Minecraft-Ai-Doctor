# Hacktoberfest 2026 Week 3 preparation

Project: **Minecraft Ai Doctor**. Theme: **Slow and Steady**. The filled, unpublished DEV draft is [DEV_POST.md](DEV_POST.md).

## GitHub creation form

| Setting | Value |
| --- | --- |
| Owner | Your account shown in the form |
| Repository name | `Minecraft-Ai-Doctor` |
| Description | Native Rust app for Minecraft crash-log analysis, mod compatibility checks, optional Gemma explanations, and reversible repair copies. |
| Visibility | Public |
| Template | No template |
| Add README | Off |
| Add .gitignore | No .gitignore |
| Add license | No license on this form; the local `LICENSE` already provides MIT licensing |

The local README, ignore rules and license should be uploaded together with the reviewed source. GitHub recommends leaving initialization options empty when importing existing code: [creating a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/creating-a-new-repository).

Repository: [Michaello2000/Minecraft-Ai-Doctor](https://github.com/Michaello2000/Minecraft-Ai-Doctor). Its public visibility and empty initial state were verified before the authorized initial upload. The post uses that repository's DEV embed and a reviewed screenshot from a synthetic fixture. Optional repository topics: `minecraft`, `rust`, `egui`, `gemma`, `hacktoberfest`.

## Submission checks

The [official challenge page](https://dev.to/challenges/hacktoberfest-week3-2026-10-19) was checked on October 25, 2026. It lists October 19-25 as the challenge window, requires a new project built around open-source AI, and asks for a DEV post with a demo. Use `devchallenge` and `hf26challenge`; the official template pre-fills those tags. The rules at the bottom specify October 25, 2026 at 11:59 PM PDT as the deadline. The page extraction shows both Live and Ended, so verify that its submission form still accepts entries.

The user has not yet recorded a video or confirmed a successful live Gemma request. The draft says this explicitly and provides a screenshot plus a reproducible synthetic example. Before submitting, record a real demo/video as requested by the template, and verify the live open-weight AI integration. Do not replace mocked tests with a claim of live inference. No API key is needed in the post, source, video or GitHub settings.

**Best Use of Gemma** is the relevant potential partner category. The challenge accepts provider-hosted Gemma, but enter it only when genuine use can be shown. The optional Prize Categories section is omitted from the draft until then. No public DevRelay session exists, so the optional My Agent Session section is also omitted. Do not upload an unreviewed chat/session containing personal paths, logs or keys.

The app is tested against real local logs and synthetic repair fixtures; no measured time-saving claim is available. Repair copies contain mods and backups, not complete runnable launcher instances. The draft keeps these limits explicit.

Read the challenge's eligibility and [official rules](https://dev.to/page/official-hackathon-rules) before submitting. The challenge page says changes committed after the deadline must be noted in the README. The user authorized the source upload; the DEV post remains an unpublished draft and no challenge entry is submitted automatically.

## Source checks

Run `python scripts/check-publish.py` after the rename and again after staging. Follow [docs/PUBLISHING.md](docs/PUBLISHING.md). Keep binaries, private exports, personal logs and API keys excluded. The renamed app deliberately keeps its legacy `Prism AI Doctor` credential-service name and `PrismAIDoctor` data directory so existing saved keys, cached responses and repair backups remain available.
