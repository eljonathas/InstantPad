<p align="center">
  <img src="assets/social-preview.png" alt="InstantPad" width="640">
</p>

# InstantPad

InstantPad is a soundboard that looks like a hardware sampler and has MyInstants built in. It runs on Windows, macOS and Linux. It mixes your microphone with the sounds you trigger and sends the result to a virtual device, which your game or Discord uses as its microphone.

```
microphone ─┐
            ├─► InstantPad ─► virtual cable ─► game / Discord
sounds ─────┤
            └─► your headphones (sounds only)
```

## Install

Download the file for your system from the [latest release](https://github.com/eljonathas/InstantPad/releases/latest).

### Windows

1. Run the `.exe` setup. It installs for your user, without an admin prompt. It is not code signed yet, so Windows may warn you the first time. Choose **More info**, then **Run anyway**.
2. Open InstantPad, go to **Settings** and click **Install VB-CABLE**. Then click **Install Driver** in the window that opens.
3. Restart your PC. The driver does not work properly until you do.
4. VB-CABLE sets itself as the Windows output device when it installs. Open the Windows sound settings and pick your headphones as the output again. InstantPad warns you in **Settings** while this is wrong.

If you prefer not to install anything, `InstantPad-portable.exe` runs on its own. It does not update itself.

### macOS

1. Open the `.dmg` and drag InstantPad to Applications. The app is not notarized yet, so macOS blocks it the first time. Open **System Settings**, go to **Privacy & Security** and choose **Open Anyway**.
2. Allow the microphone when asked.
3. Install [BlackHole 2ch](https://existential.audio/blackhole/), the virtual cable. **Install** in **Settings** takes you there.

### Linux

Run the `.AppImage`. InstantPad creates its own virtual cable through PulseAudio or PipeWire, so you need `pactl` on your system.

## Set up your game

In the voice settings of your game or Discord, choose the device **Settings** tells you as the microphone: `CABLE Output` on Windows, `BlackHole 2ch` on macOS and `InstantPad_Mic` on Linux.

If the game uses push-to-talk, hold the key while a sound plays, or switch to open mic.

In Discord, set **Input Profile** to **Studio**. Voice Isolation removes everything that is not a voice, including your sounds. If you use a custom profile, turn off noise suppression, echo cancellation and automatic gain control.

## If the sound is robotic, distorted or missing

To see whether the sound reaches the cable on Windows, open the Windows sound panel, go to **Recording** and watch the level bar of `CABLE Output` while a pad plays. If the bar moves, the problem is in the game or Discord settings. If it stays still, the problem is on the PC.

Check these in order:

1. Did you restart the PC after installing VB-CABLE? Windows shows the driver as working before the restart, but it is not.
2. In Device Manager, look for a `VB-Audio Virtual Cable` entry with an error. If there is one, or if the Windows audio service keeps stopping, reinstall the driver: run `VBCABLE_Setup_x64.exe` as administrator, click **Remove Driver**, restart, run it again, click **Install Driver**, and restart.
3. The system output device must be your headphones, not the cable. Otherwise every app on your PC goes into your microphone.
4. Bluetooth headsets drop to call quality whenever an app uses their microphone. Use a wired or USB microphone, and make sure Discord and your game are not reading the headset's mic.
5. Right-click `CABLE Input` and `CABLE Output` in the Windows sound panel and check that both use the same sample rate. VB-CABLE distorts when they differ. 48000 Hz works well.
6. Lower the **Mic** and **Game** knobs until the meter stays out of the red.

## Using it

| To | Do this |
| --- | --- |
| Play a sound | Click its pad or press its shortcut |
| Fill a pad | Click an empty pad, then **+ Pad** on a result, or drag a result onto the pad. The sound is downloaded and kept locally |
| Try an online sound on your headphones | **▶** on the result |
| Play an online sound in the game without saving it | **Mic** on the result |
| Save a copy of a sound elsewhere | Right-click its pad. Sounds on pads and favorites are already kept locally |
| Mark a favorite | **☆** on a result, or right-click a pad |
| Use your own file | Drag it from Explorer onto a pad, or right-click the pad |
| Change a pad's shortcut, volume, color or name | Right-click the pad |
| Rearrange pads | Hold Ctrl and drag |
| Turn a knob | Drag up or down, or use the mouse wheel. Double-click resets it |

The knobs set your voice in the game (**Mic**), the sounds in the game (**Game**) and the sounds you hear (**Headphones**).

Only one copy of the app runs at a time. Opening it again brings the existing window to the front.

### Interfaces

Create as many interfaces as you want with **+**. Each has its own name, color and grid, up to 8 rows by 10 columns. The gear button configures the current one, and right-clicking a tab moves or deletes it.

### Numeric shortcuts

These are on by default and work while a game has focus:

| Key | Action |
| --- | --- |
| `Num1` to `Num9`, `Num0` | The first 10 pads of the active interface |
| `Num+` / `Num-` | Next / previous interface |
| `Num,` | Stop everything |

**Settings** can switch them to `Ctrl` plus the numeric keypad, or `Alt` plus `1` to `0`, or turn them off. Keep NumLock on. A shortcut you set on a pad takes priority over these.

### Language

The app follows your system language and comes in English and Portuguese. You can change it in **Settings**.

## Automatic updates

InstantPad looks for a new version when it opens and downloads it in the background, checking its signature. The new version installs when you close the app, or right away with **Restart now** in **Settings**. It never restarts by itself, so an update cannot interrupt a match.

## Development

You need Rust, Node 22 and pnpm. On Linux you also need the [Tauri system libraries](https://tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm dev
```

```bash
cargo test --workspace
pnpm check && pnpm lint && pnpm test
```

All state and rules live in Rust, and the React interface draws what the backend sends:

```
apps/desktop/src-tauri        Tauri shell, commands, composition root
   └─► crates/audio, catalog, storage, platform     adapters
          └─► crates/application     use cases and ports
                 └─► crates/domain   pad board and hotkey rules
```

See [docs/architecture](docs/architecture/README.md) and [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

## Limitations

On Windows the virtual device is VB-CABLE, from VB-Audio. Windows only loads audio drivers signed by Microsoft, so the app cannot create its own. It downloads the setup from the official site and runs it only if the digital signature belongs to VB-Audio. VB-CABLE is free, with an optional donation.

MyInstants has no public API, so the app reads the site's HTML. If the site changes, [`parser.rs`](crates/catalog/src/parser.rs) is the file to fix.

A game running as administrator can block global shortcuts. If that happens, run InstantPad as administrator too.

Some keys of Brazilian ABNT2 keyboards, such as the `/` next to the right Shift, cannot be global shortcuts on Windows.

The virtual cable on Linux stays after you close the app, until you log out, and the next start reuses it.

## License

[MIT](LICENSE). The app uses the [Lexend](https://www.lexend.com/) font under the [SIL Open Font License](apps/desktop/src/shared/styles/fonts/OFL.txt).
