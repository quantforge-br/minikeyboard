# Mini Keyboard

Native Linux programmer for USB mini macro keyboards.

Stack: **Rust 2024**, **GTK 4**, **libadwaita**, **hidapi** (`linux-native`).  
Application ID: `br.com.quantmind.MiniKeyboard`  
License: **GPL-3.0-or-later**

## Requirements

- Rust stable (edition 2024)
- GTK 4 ≥ 4.10 development packages
- libadwaita ≥ 1.5 development packages
- hidapi / libudev headers
- `glib-compile-resources`

On Arch Linux:

```bash
sudo pacman -S gtk4 libadwaita hidapi pkgconf
```

## Build & run

```bash
make              # same as: make build
make build        # cargo build --release → target/release/minikeyboard
cargo run
```

Debug logging:

```bash
RUST_LOG=minikeyboard=debug cargo run
```

### UI smoke scenarios (no hardware)

```bash
MINIKEYBOARD_MOCK_SCENARIO=disconnected cargo run
MINIKEYBOARD_MOCK_SCENARIO=known cargo run
MINIKEYBOARD_MOCK_SCENARIO=unknown cargo run
MINIKEYBOARD_MOCK_SCENARIO=dirty cargo run
```

Experimental VID/PID pairs (read-only):

```bash
MINIKEYBOARD_EXPERIMENTAL_DEVICES=1 cargo run
```

## Install udev rule (required for hardware)

```bash
sudo ./linux/setup-hid-permissions.sh
# reconnect the device, then:
udevadm info /dev/hidrawN | grep -E 'ID_VENDOR_ID|ID_MODEL_ID|TAGS'
```

Uninstall:

```bash
sudo ./linux/setup-hid-permissions.sh --uninstall
```

The rule grants `uaccess` only for `1189:8842`. It never uses `MODE="0666"`.

## Tests

```bash
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

## Packaging

Developer builds use Cargo (`make` / `make build`). Distro install uses Meson.
`make install` builds as your user, then elevates only the install step (password prompt). Staging with `DESTDIR` stays unprivileged:

```bash
make install                         # PREFIX=/usr/local — prompts sudo
PREFIX=/usr make install             # system prefix — prompts sudo
make uninstall                       # removes last install (prompts sudo when needed)
DESTDIR="$PWD/stage" make install    # staged tree — no sudo
```

Flatpak (Devel):

```bash
# Generate crate sources after Cargo.lock exists:
#   flatpak-cargo-generator.py Cargo.lock -o build-aux/cargo-sources.json
flatpak-builder --user --install build-dir build-aux/br.com.quantmind.MiniKeyboard.Devel.json
```

Host udev installation is still required for hardware access from Flatpak (`--device=all`).

## Applications & scripts (launcher daemon)

The "Applications and Scripts" editor block binds a shell command to a key: the
key receives a marker chord (F13–F24 + modifiers) and `minikeyboard-daemon`
watches the device's input nodes and launches the command when the marker
fires. Bindings live in `~/.config/minikeyboard/bindings.json` and reload
automatically.

The daemon may start before the graphical login. Each binding therefore runs as
a transient `systemd --user` unit, inheriting the manager's current graphical
environment (`DISPLAY`, `WAYLAND_DISPLAY`, and D-Bus) instead of the daemon's
boot-time environment. Script exit status and stderr are recorded by
`journalctl --user -u minikeyboard-daemon`.

A real Meson install (`make install`, `DESTDIR` unset) fully enables autostart:

- Meson installs an enablement symlink (`WantedBy=default.target`) so the unit
  is enabled for the user manager;
- `make install` then enables lingering for the target user and **restarts** the
  daemon immediately, so it also comes up at boot without a login session and
  a reinstall always replaces a running older binary.

Under `sudo`, the target user is the invoking `SUDO_USER` (never root); a
root shell must pass `TARGET_USER=<user>` explicitly. Override the account with
`make install TARGET_USER=<user>`.

A staged/packaging install (`DESTDIR=...`) deliberately installs only the
enablement symlink and performs no linger/start — the packaged system decides
those at first boot.

A developer build without `make install` still needs the manual one-time
`systemctl --user enable --now minikeyboard-daemon` (plus `loginctl
enable-linger "$USER"` for boot without login).

The daemon needs read access to `/dev/input/event*` (the `input` group).

## Protocol safety

Configuration reading is validated for physical hardware tuple `1189:8842` using the captured
three-pass `0xFA` exchange. Every read preserves the returned record bytes and records a SHA-256
provenance digest.

Writes remain fail-closed:

- unknown and experimental device tuples stay read-only;
- imported or device-read records preserve opaque bytes;
- an action encoder is enabled only after its write/readback mapping is validated;
- commit is sent once, then configuration is read back before success is reported;
- timeout or disconnect after commit is reported as indeterminate and is never retried automatically.

Current hardware validation covers reading only. Action encoders, including clear/reset, remain
disabled until explicit reversible write/readback captures are imported and reviewed.

Capture tooling: `tools/capture-usbmon.sh`, `cargo run --bin import-vector`, and the read-only
hardware smoke command `cargo run --bin read-probe`.

## Historical / reference downloads

The following URLs are the manufacturer's original packages, retained only as
reference for clean-room observation. **They are not part of this product** and
must not be bundled or linked into the native binary:

- http://www.szxiaozi.com/WINEN.rar
- http://www.szxiaozi.com/MACEN.zip
