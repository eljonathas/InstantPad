<p align="center">
  <img src="assets/social-preview.png" alt="InstantPad" width="640">
</p>

# InstantPad

InstantPad is a soundboard for Windows that looks like a hardware sampler and has MyInstants built in. It mixes your microphone with the sounds you trigger and sends the result to a virtual device, which your game or Discord uses as its microphone.

```
microphone ─┐
            ├─► InstantPad ─► CABLE Input ═ driver ═ CABLE Output ─► game / Discord
sounds ─────┤
            └─► your headphones (sounds only)
```

## Install

1. Download `InstantPadSetup.exe` from the [latest release](https://github.com/eljonathas/InstantPad/releases/latest) and run it. It installs for your user, without an admin prompt, and adds a desktop shortcut and a Start menu entry. The file is not code signed yet, so Windows may warn you the first time. Choose **More info**, then **Run anyway**.
2. Open InstantPad, go to **Settings** and click **Install VB-CABLE**. Then click **Install Driver** in the window that opens.
3. Restart your PC. The driver does not work properly until you do.
4. VB-CABLE sets itself as the Windows output device when it installs. Open the Windows sound settings and pick your headphones as the output again. InstantPad warns you in **Settings** while this is wrong.

If you prefer not to install anything, `InstantPad.exe` from the same release runs on its own.

## Set up your game

In the voice settings of your game or Discord, choose `CABLE Output` as the microphone.

If the game uses push-to-talk, hold the key while a sound plays, or switch to open mic.

In Discord, set **Input Profile** to **Studio**. Voice Isolation removes everything that is not a voice, including your sounds. If you use a custom profile, turn off noise suppression, echo cancellation and automatic gain control.

## If the sound is robotic, distorted or missing

To see whether the sound reaches the cable, open the Windows sound panel, go to **Recording** and watch the level bar of `CABLE Output` while a pad plays. If the bar moves, the problem is in the game or Discord settings. If it stays still, the problem is on the PC.

Check these in order:

1. Did you restart the PC after installing VB-CABLE? Windows shows the driver as working before the restart, but it is not.
2. In Device Manager, look for a `VB-Audio Virtual Cable` entry with an error. If there is one, or if the Windows audio service keeps stopping, reinstall the driver: run `VBCABLE_Setup_x64.exe` as administrator, click **Remove Driver**, restart, run it again, click **Install Driver**, and restart.
3. The Windows output device must be your headphones, not `CABLE Input`. Otherwise every app on your PC goes into your microphone.
4. Bluetooth headsets drop to call quality whenever an app uses their microphone. Use a wired or USB microphone, and make sure Discord and your game are not reading the headset's mic.
5. Right-click `CABLE Input` and `CABLE Output` in the Windows sound panel and check that both use the same sample rate. VB-CABLE distorts when they differ. 48000 Hz works well.
6. Lower the **Mic** and **Game** knobs until the meter stays out of the red.

## Using it

| To | Do this |
| --- | --- |
| Play a sound | Click its pad or press its shortcut |
| Fill a pad | Click an empty pad, then **+ Pad** on a result, or drag a result onto the pad |
| Try an online sound on your headphones | **▶** on the result |
| Play an online sound in the game without saving it | **Mic** on the result |
| Save a sound to your computer | **↓** on a result or favorite, or right-click a pad |
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

InstantPad looks for a new version when it opens and downloads it in the background, checking its SHA-256. The new version starts the next time you open the app, or right away with **Restart now** in **Settings**. It never restarts by itself, so an update cannot interrupt a match.

## Development

You need the .NET 8 SDK.

```bash
dotnet test InstantPad.sln
```

```bash
dotnet run --project src/InstantPad.Desktop
```

The code follows Clean Architecture, with dependencies pointing inwards:

```
InstantPad.Desktop                 WPF, view models, composition root
   └─► InstantPad.Infrastructure   NAudio, MyInstants, GitHub, JSON files, driver
          └─► InstantPad.Application   use cases, ports, translated text
                 └─► InstantPad.Domain     pad board and hotkey rules
```

Every push to `main` runs the tests and publishes a release. The version comes from the [Conventional Commits](https://www.conventionalcommits.org/) since the last tag. See [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

## Limitations

The virtual device is VB-CABLE, from VB-Audio. Windows only loads audio drivers signed by Microsoft, so the app cannot create its own. It downloads the setup from the official site and runs it only if the digital signature belongs to VB-Audio. VB-CABLE is free, with an optional donation.

MyInstants has no public API, so the app reads the site's HTML. If the site changes, [`MyInstantsPageParser.cs`](src/InstantPad.Infrastructure/Catalog/MyInstantsPageParser.cs) is the file to fix.

A game running as administrator can block global shortcuts. If that happens, run InstantPad as administrator too.

## License

[MIT](LICENSE). The app uses the [Lexend](https://www.lexend.com/) font under the [SIL Open Font License](src/InstantPad.Desktop/Assets/Fonts/OFL.txt).
