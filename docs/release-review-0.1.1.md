# 0.1.1 release hold and verification

Status: unreleased. Do not treat source changes or passing tests as antivirus clearance.

## Confirmed installation failure

On September 5, 2026, Windows Defender detection history identified the installed v0.1.0
`lossy.exe` as `Trojan:Win32/Bearfoos.A!ml`. Successful remediation records included the
executable, desktop and Start menu shortcuts, startup Run entry, and uninstall registration.
This explains why the app stopped capturing and disappeared from Windows Installed Apps.
No archive deletion was observed. Raw local detection logs and personal paths are not published.

The release installer SHA-256 is
`48784cd8d5dfe82a871bf62b82613222188a0b63f9928be2a87a61b0defa5e19`.
A later custom, non-remediating scan with Defender platform 4.18.26080.3-0 and intelligence
1.459.64.0 reported no threats in that installer and in the revised debug executable.
These file-only scans do not clear the earlier installed-app detection or establish a false positive.

## Implemented source fixes

- Lossy installer/uninstaller icons, publisher/help metadata, and branded footer.
- Verify installed executable, companion resources, Windows registration and both shortcuts.
- Preserve integrations during updater uninstall; repair previously enabled startup and browser
  registrations after installation without showing a window or changing capture consent.
- Opt-in broader desktop scope plus an editable selected-app list; existing scope stays intact.
- Match full executable paths for same-app renderer accessibility, instead of requiring one PID.
- Visible last-save, native-field and clipboard diagnostics with no prompt contents in diagnostics.
- Retry native snapshots that were not acknowledged by storage; retain size and sensitive-field guards.
- Verify clipboard ownership and recheck focus/sequence; ownerless copies remain unsupported.

## Release gates still required

Local source verification: 36 Rust tests, Clippy, frontend build, synthetic browser arrangement/
popup/setup flows, and the revised debug agent's interactive native/clipboard/password/restart
smoke test passed. Native fixture captures only an explicitly allowed synthetic editor and uses
an isolated temporary archive. This is not final-installer or physical-power-loss certification.

1. Submit the public release binary/installer to Microsoft as a software developer and wait for
   its final determination. This requires the maintainer's submission approval and sign-in.
   [Microsoft developer guidance](https://learn.microsoft.com/en-us/defender-xdr/developer-faq).
2. Investigate any confirmed malicious cause; do not label a detection false merely because
   this is our own project or because a later scan is clean.
3. Verify the final packaged installer on a clean Windows account with Defender active:
   install, first-run consent, native/clipboard/companion smoke tests, real application UI,
   process restart, sign-in startup, upgrade and uninstall/discovery checks.
4. Complete CI and review. Only then merge for release, tag, and publish checksummed artifacts.

No Defender exclusions, protection changes, quarantined-file restorations, or private archive
uploads are part of this workflow. Code signing is desirable publisher verification, not a
substitute for resolving an antivirus detection.
