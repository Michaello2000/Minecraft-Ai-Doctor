# Lunar Client log import

The Doctor discovers folders beneath `%USERPROFILE%\.lunarclient\profiles`. Lunar layouts can change. A typical log is `%USERPROFILE%\.lunarclient\profiles\1.21\logs\latest.log`; select it manually if discovery does not match your installation. A `logs` directory itself can also be imported.

For boot failures, import `ichor-boot.log` or a launcher `.log`/`.txt` explicitly. Profile import reads `logs/latest.log` and `logs/ichor-boot.log` if present. Compressed historical logs are not supported. Do not import configuration/account files; they are not needed.

If a profile exposes top-level JARs in `mods`, they can be inspected as metadata. Lunar's private/bundled components may not expose ordinary descriptors; log-only diagnosis remains useful, and missing metadata is shown as unknown. The app does not hook into Lunar internals, unlock mod support, bypass restrictions or promise an official plugin API.

Lunar logs may include account names, server addresses and session information. Offline import stays local. Review the redacted payload before each cloud request.
