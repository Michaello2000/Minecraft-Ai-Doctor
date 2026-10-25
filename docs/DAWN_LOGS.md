# Dawn profiles

The Doctor enumerates `%APPDATA%\.dawn\profiles` when it exists and displays those directories under the **Dawn** launcher tab. A selected profile can expose `.minecraft/logs/latest.log`, `.minecraft/crash-reports` and `.minecraft/mods`; the existing bounded log/JAR readers handle them read-only.

This layout was confirmed on the development PC, but it is a discovery heuristic, not a public Dawn integration API. For portable or changed layouts, use **Open a folder**, a custom root, or manual log import. No Dawn code is executed, hooked, modified or unpacked into the instance.

Log-only imports and pasted console text still work when mod or runtime metadata is unavailable. Unknown game/loader details must be corrected before requesting an online release match. A matching published release does not establish compatibility with client-specific patches.
