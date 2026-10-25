---
title: "Minecraft Ai Doctor: spend less time fixing your modpack"
published: false
tags: devchallenge, hf26challenge, minecraft, rust
---

*This is a submission for the [Hacktoberfest Open-Source AI Challenge Week 3: Slow and Steady](https://dev.to/challenges/hacktoberfest-week3-2026-10-19)*

## What I Built

Adding one Minecraft mod can turn a working pack into a wall of errors. I built **Minecraft Ai Doctor** to make that wall easier to understand.

Select a Prism, Lunar or Dawn profile, import a log, or paste console output. The app groups missing dependencies, duplicate mods and version conflicts, points to the original evidence, and explains what to check next. It can look up published mod releases and apply reviewed changes to a separate mod-only repair copy, with rollback. Your original instance stays untouched.

## Demo

![Minecraft Ai Doctor explaining a Nvidium and Sodium version conflict from a synthetic test log](https://raw.githubusercontent.com/Michaello2000/Minecraft-Ai-Doctor/main/assets/demo/diagnosis.png)

This is a native Windows app. The preview shows a synthetic Nvidium/Sodium conflict and the evidence-based steps it produces. To try that example from the source:

```powershell
cargo run --release --locked -- --log tests/fixtures/sodium-conflict.log
```

A video walkthrough has not been recorded yet.

## Code

The source is MIT licensed:

{% github Michaello2000/Minecraft-Ai-Doctor %}

## How I Built It

The app uses Rust and egui. Local rules read bounded logs and mod metadata without executing JARs. The optional AI layer has adapters for Google's hosted Gemma and Qwen through Groq. Cloud requests require a preview of the redacted evidence and explicit consent; repeated identical requests can reuse cached results.

AI citations must refer to supplied evidence. File changes come from locally checked operations that the user reviews, rather than model-generated commands. The current build passes 42 automated tests. The provider adapters have been tested with mocked responses; live Gemma validation is still pending.

## Why Does Open Innovation Matter?

I wanted the diagnosis rules and repair behavior to be inspectable. Anyone can read the source, add a rule or replace a provider adapter. Open-weight models leave room for future self-hosting and adaptation instead of tying the design to one model. This MVP uses hosted APIs for AI; local inference is not implemented. The offline checks work without an API key.

## The Lazy Payoff

I used real Prism and Lunar logs during development, including a Nvidium/Sodium version conflict. The payoff is a clearer starting point instead of jumping between console output, mod folders and release pages. I have not measured the time saved yet, and a matching release still needs testing in a disposable game instance.
