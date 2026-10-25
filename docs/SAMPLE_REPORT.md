# Minecraft Ai Doctor

Read-only local analysis. Metadata checks cannot guarantee runtime compatibility.

## Required mod dependency is missing or unresolved [error]
Deterministic signal; root cause may need confirmation

Install fabric-api (>=0.100)

1. Create a disposable copy of this instance in your launcher before changing mods or runtime settings.
2. Example requires fabric-api, version >=0.100. On that mod's official page, choose a build for this copy's exact Minecraft version and loader.
3. Add the matching fabric-api JAR to the copy's mods folder. Keep a single intended version of that mod.
4. Launch the copy once, then import its new latest.log here to check whether this failure is gone.

Needs confirmation: The log supplies the dependency and version requirement. A compatible download has not been verified or downloaded.


- .\tests\fixtures\missing.log:2 — [12:00:01] [main/ERROR]: Incompatible mods found!

- .\tests\fixtures\missing.log:3 — net.fabricmc.loader.impl.discovery.ModResolutionException: Mod 'Example' (example) requires version >=0.100 of fabric-api, which is missing!

- .\tests\fixtures\missing.log:4 — 

Warning: Mod metadata describes declared requirements only. Nested JARs, aliases, dynamic loader behavior and unsupported version syntax can leave results unknown.

