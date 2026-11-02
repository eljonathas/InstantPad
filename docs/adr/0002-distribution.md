# 0002: Distribution

Releases come from the `Release desktop` workflow, started by hand with a version. It builds Windows, macOS (Apple silicon and Intel) and Linux, and leaves a draft release to review before it goes public.

- **Updates** use `tauri-plugin-updater` with a Minisign key. The public key is in `tauri.conf.json`. The private key is the `TAURI_SIGNING_PRIVATE_KEY` secret, and losing it means installed apps stop accepting updates.
- **An update never interrupts a match.** It downloads in the background, then applies on **Restart now** or when the app closes.
- **The portable exe** is published as `InstantPad-portable.exe`. Its name turns the updater off, because nothing installed it.
- **macOS builds are not notarized**, since there is no Apple Developer account yet. They are only signed ad hoc, so people must allow the app in the system settings the first time they open it.
- **The installers set up the virtual cable.** The Windows setup runs the app with `--check-cable` and, when the cable is missing, offers `--install-cable`, the same download and signature check as the Install VB-CABLE key in Settings. Only that step asks for administrator rights, and it waits for the VB-Audio setup to close so the finish page can offer the restart the driver needs. An update runs the setup passively and skips it. On macOS a `.dmg` has no install step, so releases also carry a `.pkg` whose BlackHole choice, ticked when the cable is missing, installs the official package pinned by its SHA-256. The driver is downloaded rather than bundled, so a release never redistributes it.
- **No bridge release:** there are no active users, so the C# version does not lead people to the new installer.
