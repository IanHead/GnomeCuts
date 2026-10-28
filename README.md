# GnomeCuts

GNOME-style workspace shortcuts for Windows 11's virtual desktops. Runs in the system tray.

| Shortcut | Action |
|---|---|
| Win+PgUp / Win+PgDn | Switch to the previous / next desktop |
| Win+Shift+PgUp / Win+Shift+PgDn | Move the focused window to the previous / next desktop and follow it. Moving past the last desktop creates a new one. |
| Push the mouse into the top-left corner of the main display | Open Task View, or close it if it's already open |
| Scroll over the Start or Task View button | Wheel up: previous desktop. Wheel down: next desktop. |
| Win + scroll anywhere | Wheel up: previous desktop. Wheel down: next desktop. |

The hot corner follows whichever display is set as the main display in Settings > System > Display. If another monitor sits to the left of or above that corner, the cursor slides past it instead of stopping. Moving the cursor up or left through the corner still triggers it.

## Build and run

You need Rust (MSVC toolchain) and the Visual Studio Build Tools.

```
cargo build --release
target\release\gnomecuts.exe
```

Right-click (or left-click) the tray icon to turn the hot corner, button scrolling and Win+scroll on or off, to start GnomeCuts with Windows, or to exit. Settings are stored in `HKCU\Software\GnomeCuts`.

The exe icon is `assets/gnomecuts.ico`, which is embedded at build time. To change it, edit and run `python assets/make_icon.py`.

`gnomecuts.exe --diag` prints the desktop count and where it found each taskbar and its Start and Task View buttons. Use it when something isn't working.

## How it works

- **Switching** sends Windows' own Ctrl+Win+Left/Right, so you get the normal animation.
- **Moving windows** uses [winvd](https://crates.io/crates/winvd), which calls Windows' undocumented virtual desktop COM interfaces. Microsoft changes these interfaces between Windows builds, so a Windows update can break moving windows until winvd is updated. Switching desktops, the hot corner and scrolling don't use these interfaces.
- **Button scrolling** uses UI Automation to find the Start and Task View buttons on each Windows 11 taskbar. A background thread keeps their positions up to date, because these buttons move on a centred taskbar when apps open and close. If you've hidden the Task View button in taskbar settings, only the Start button responds.
- A low-level mouse hook watches for the hot corner, for scrolling over these buttons, and for scrolling while Win is held. After Win+scroll it taps an unassigned key so that releasing Win doesn't open the Start menu.

## Limitations

- Switching desktops and the hot corner may not work while an app running as administrator has focus, unless GnomeCuts is also run as administrator. Both work by sending simulated keypresses, and Windows blocks those from reaching elevated apps.
- The hot corner doesn't fire while a mouse button is held down or while a fullscreen app has focus.

## Licence

MIT. See [LICENSE](LICENSE).
