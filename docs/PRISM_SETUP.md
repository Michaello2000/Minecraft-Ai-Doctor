# Prism Launcher integration

Use manual import first. The standard Windows data directory is `%APPDATA%\PrismLauncher`; portable installs can use another root. The app's custom-root field expects the **instances** directory. Select an individual instance in the folder picker to analyze it directly.

Prism can provide `INST_MC_DIR`, `INST_DIR` and `INST_NAME` to custom commands. The app uses the Minecraft directory first and falls back to the instance directory. Missing variables produce a clear error; no hook environment is assumed in manual mode. Instance names are not interpolated into shell commands or sent automatically to a provider.

In Prism's per-instance Settings → Custom Commands, review the existing post-exit command. Do not overwrite it without preserving and reviewing its behavior. To open the Doctor after every exit, set a command using your actual absolute executable path:

```text
"D:\Tools\Minecraft Ai Doctor\minecraft-ai-doctor.exe" --from-prism
```

For fresh log failure signals only:

```text
"D:\Tools\Minecraft Ai Doctor\minecraft-ai-doctor.exe" --from-prism --only-crashes --max-log-age-seconds 300
```

The direct command needs no `%INST_MC_DIR%` expansion: the Doctor reads the inherited environment. Quote the executable path when it contains spaces. Do not paste PowerShell's leading `&` into Prism's command field. Do not use `cmd /c` or a wrapper merely to expand `%` variables. For an existing custom command, combine behavior manually only after reviewing the launcher's command semantics; the Doctor never edits launcher settings.

`--only-crashes` checks `latest.log` modification age and recognized error signals. It does not infer a crash from the hook firing, and does not treat an old crash report as a current crash. Exit status is not assumed: Prism may not provide one to the hook. Recoverable errors can still trigger opening, and unrecognized crashes can be missed. Use every-exit mode or manual import if you need to inspect all launches.

Instance import reads top-level `mmc-pack.json`, providing game/loader versions for dependency comparison. When `minecraft` or `.minecraft` is selected (including the hook path), its immediate parent's `mmc-pack.json` is also recognized. Missing runtime versions remain unknown.

No automatic hook installer, launch control, game execution or instance modification is included.

References: [custom commands](https://prismlauncher.org/wiki/help-pages/custom-commands/), [environment variables](https://prismlauncher.org/wiki/help-pages/environment-variables/), [data locations](https://prismlauncher.org/wiki/getting-started/data-location/).
