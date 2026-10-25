# Compatibility Lab (experimental)

This release includes a read-only planning harness. Select a Minecraft/mods folder in the Lab and build an inventory, or run:

```powershell
.\dist\minecraft-ai-doctor.exe lab-plan --instance 'D:\Disposable\Test Pack'
```

The result records sorted JAR filenames and SHA-256 hashes, plus manual experiment steps. It never copies or modifies the selected instance. An inventory of a real instance does not make it disposable. The UI does not assert otherwise.

To perform a live experiment yourself:

1. Create a dedicated disposable instance, excluding saves, credentials and unrelated personal data. Keep the original untouched.
2. Record Minecraft, loader, pack name/version, all mod versions and hashes. Keep a pristine baseline copy.
3. With separate approval for account-bound game execution, run the baseline and import its log.
4. Obtain approval before each download, mod change and launch. Use trusted sources with compatible distribution terms; verify published checksums.
5. Add exactly one named mod/version. Record URL/hash, change, expected outcome and actual result. Import the new log and compare evidence.
6. Restore the disposable copy to its baseline, recheck hashes and repeat only with explicit scope.

No downloader, random mutation, instance creator, automated game launcher or rollback executor is implemented. Offline ZIP/log fixtures cover parser behavior, declared conflicts and duplicates; they do not prove that a real modpack launches. No live Fabulously Optimized experiment is claimed.
