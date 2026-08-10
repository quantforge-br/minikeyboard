# Specification — Native Linux Mini Keyboard Programmer

Status: clean-room implementation proposal based on observable behavior, artifacts distributed by the manufacturer, and USB/HID descriptors of the purchased hardware.

## 1. Objective

Create a native Linux application to configure the USB macro keyboards currently served by the `MINI_KEYBOARD.exe` application, without Wine, without packaging Windows binaries, and without reusing manufacturer code.

The application must:

- discover the device by USB VID/PID and select only its configuration interface;
- automatically identify the keyboard geometry;
- read the configuration stored on the device;
- edit keys, macros, multimedia, mouse, delay, layers, and LED/RGB;
- write only the modified positions;
- verify the write by re-reading;
- import and export local profiles;
- work without `root`, using a udev rule restricted to the device.

## 2. Clean-room contract

### 2.1 Permitted sources

- USB and HID descriptors exposed by the device itself;
- USB/HID traffic produced by legally operating the hardware and the original application;
- names, text, and visible behavior of the interface;
- public documentation of USB HID, hidapi, Linux hidraw, GTK, and Rust;
- this behavioral specification document.

### 2.2 Restrictions

The implementation must not:

- copy or translate decompiled code from the original application;
- embed `MINI_KEYBOARD.exe`, DLLs, COFF objects, or the current AppImage;
- reproduce images, visual identity, or graphical assets from the manufacturer;
- depend on Wine;
- grant global access to all `/dev/hidraw*` devices;
- enable writing to unknown devices simply because they use HID.

### 2.3 Recommended separation

1. **Protocol team:** collects captures, produces test vectors, and updates only this specification.
2. **Implementation team:** receives the specification and the vectors, but does not consult decompilation or manufacturer code.
3. **Review:** confirms that each implementation can be explained by a requirement of this specification or by public documentation.

## 3. Observed evidence

### 3.1 Distributed application

- Main application: PE32/i386, Qt 5.14.2, and MinGW.
- Transport: hidapi.
- The existing `MiniKeyboard-x86_64.AppImage` is not a native Linux application. Its SquashFS contains the Windows executable, Qt DLLs for Windows, `hidapi.dll`, and the same objects from the Windows package.
- The application polls the connection approximately every 100 ms.

### 3.2 Confirmed hardware

Device connected during the analysis:

| Field | Value |
|---|---|
| VID | `0x1189` |
| PID | `0x8842` |
| USB Product | `USB Composite Device` |
| Observed serial | `CD70134330393835` |
| USB | 1.10, Full Speed |

The device has two HID interfaces:

| Interface | Purpose | Endpoints |
|---|---|---|
| 0 | Proprietary configuration | OUT `0x04`, IN `0x84`, 64 bytes |
| 1 | Keyboard, mouse, and consumer control | IN `0x82`, 16 bytes |

Interface 0 uses:

- Usage Page `0xFF00`;
- Report ID `0x03`;
- 64-byte input;
- 64-byte output.

The application must open interface **0**, never keyboard interface 1.

### 3.3 IDs queried by the original software

Observed VIDs:

- `0x1189`;
- `0x514C`.

Observed PIDs:

- `0x8842`;
- `0x8840`;
- `0x8830`;
- `0x8831`;
- `0x8832`;
- `0x8833`;
- `0x8850`;
- `0x8851`.

Only `0x1189:0x8842` was physically validated in this analysis. The remaining ones must stay marked as `experimental` until validated on hardware.

## 4. Functional model

### 4.1 Recognized variants

The identification response contains a three-byte tuple interpreted as key count, additional control count/variant, and subtype. UI routes have been observed for:

- `0+1`, `0+2`, `0+3`;
- `1+0`, `2+0`;
- `3+0`, `3+1`;
- `4+0`, `4+1`, `4+2`, `4+3`;
- `5+0` with mute;
- `6+0`, `6+1`, `6+2`;
- `9+2`, `9+3`;
- `11+3`;
- `12+2`, `12+3`, `12+4`;
- `15+3` as fallback;
- `16+0`;
- `21+1`.

Geometry must be described by data, not by screens or classes specific to each model.

### 4.2 Layers and positions

- 3 configurable layers.
- Up to 60 logical positions per layer.
- Each position occupies a 50-byte record in the observed protocol.
- The physical layout may expose fewer positions than the protocol limit.
- Each position has an independent dirty flag.

### 4.3 Required action types

1. **Basic key:** any keyboard usage supported by the firmware.
2. **Combination:** Ctrl, Shift, Alt, and GUI/Win modifiers combined with a key.
3. **Macro/sequence:** multiple events or modifier/key pairs within the record limit.
4. **Multimedia:**
   - play/pause;
   - stop;
   - previous track;
   - next track;
   - mute;
   - volume +;
   - volume -;
   - calculator.
5. **Mouse:**
   - left button;
   - middle button;
   - right button;
   - wheel + and -;
   - wheel with Ctrl, Shift, or Alt.
6. **LED/RGB:** 6 observed modes and at least 8 color selections.
7. **Delay:** value associated with the action, represented by two bytes in the record.
8. **Layer:** select and edit layers 1, 2, and 3.
9. **Clear:** clear a single position or all positions in the current layer.

## 5. HID protocol

### 5.1 Transport

- Linux backend: hidapi/hidraw.
- Selection: allowed VID/PID, Usage Page `0xFF00`, Report ID `0x03`, and `interface_number == 0` when available.
- One session per device.
- Every write must be serialized.
- The I/O worker must have a timeout and cancellation on disconnection.

The original software writes 65-byte buffers: 1 Report ID byte plus 64 bytes of report. It requests 64-byte reads. This asymmetry must be preserved initially and logged as a compatibility issue to be confirmed via USB capture.

### 5.2 Identification query

Observed transmission:

```text
03 FB FB FB 00 ... 00
```

- size passed to hidapi: 65 bytes;
- read: up to 64 bytes;
- observed timeout: 10 ms;
- response bytes at indices 2, 3, and 4 determine the physical variant.
Requirements:

- only accept a response with sufficient size for the indices used;
- reject an empty, truncated response, or one from a different Report ID;
- do not assume geometry when the tuple is unknown;
- for an unknown tuple, open in diagnostic/read-only mode.

### 5.3 Configuration Reading

Observed header:

```text
03 FA <layer-or-count> <range> <block-index> 00 ... 00
```

Observed behavior:

- the operation iterates over the 3 layers;
- each command is followed by a sequence of HID reads;
- 49 useful bytes from each response are copied to the register area;
- the original UI calls the read with parameters equivalent to `(3, 15, 3)`.

The exact semantics of bytes 2–4 still need to be finalized with a USB capture. The initial implementation must encapsulate this command in `ReadConfigCommand`, without spreading offsets throughout the application.

### 5.4 Position Writing

For each dirty position:

```text
byte 0      = 0x03              # Report ID
bytes 1–50  = position record
bytes 51–64 = 0x00
```

- total size: 65 bytes;
- clean positions must not be sent;
- the order must be ascending layer and ascending position;
- any error interrupts the transaction before commit.

### 5.5 Commit/Finalization

After the last set of positions:

```text
03 FD FE FF 00 ... 00
```

- total size: 65 bytes;
- observed wait after the commit: 200 ms;
- after the commit, the application must re-read and compare the configuration.

Do not automatically retry the commit after a timeout: the firmware may have applied the transaction despite the lack of confirmation.

### 5.6 Partial Structure of the 50-byte Record

The offsets below are observed; unconfirmed fields remain opaque.

| Offset | Size | Interpretation |
|---:|---:|---|
| 0 | 1 | marker/command; RGB uses `0xFE` |
| 1 | 1 | subtype; RGB uses `0xB0` |
| 2 | 1 | 1-based layer in the RGB command |
| 3 | 1 | main mode/type of the action |
| 4–5 | 2 | delay, byte order little-endian, to be confirmed |
| 9 | 1 | counter/length of the action |
| 10–49 | 40 | action data, pairs and specific fields |

Observed RGB:

- byte 0: `0xFE`;
- byte 1: `0xB0`;
- byte 2: layer + 1;
- byte 9: `1`;
- byte 11: mode and color packed into nibbles.

The remaining bytes must be preserved when a configuration is read and partially edited. The encoder must not zero out unknown fields without an explicit reset action.

## 6. Domain Model

```rust
struct DeviceIdentity {
    vid: u16,
    pid: u16,
    serial: Option<String>,
    path: String,
    interface_number: i32,
    usage_page: u16,
    usage: u16,
}

struct DeviceVariant {
    key_count: u8,
    extra_count: u8,
    subtype: u8,
    geometry_id: String,
    support: SupportLevel,
}

struct DeviceConfig {
    schema_version: u32,
    identity: ProfileIdentity,
    layers: [LayerConfig; 3],
    opaque_device_data: Vec<u8>,
}

struct LayerConfig {
    positions: Vec<PositionConfig>,
}

struct PositionConfig {
    logical_index: u8,
    action: Action,
    delay_ms: Option<u16>,
    raw_record: [u8; 50],
    dirty: bool,
}

enum Action {
    Empty,
    Keyboard { modifiers: Modifiers, usage: u8 },
    Sequence { strokes: Vec<Stroke> },
    Consumer { usage: u16 },
    Mouse { action: MouseAction, modifiers: Modifiers },
    Lighting { mode: u8, color: u8 },
    Opaque { mode: u8 },
}
```

Rules:

- `raw_record` is the source for lossless round-tripping.
- `Action` is the semantic view of the known fields.
- `Opaque` retains configurations that are not yet understood.
- editing a known field must only change the corresponding bytes.
- saved profiles must include VID, PID, variant tuple, and schema version.

## 7. Linux Architecture

### 7.1 Chosen Stack

- Language: stable Rust, edition 2024.
- UI: GTK 4 + libadwaita.
- HID: `hidapi` crate, hidraw backend.
- Hotplug: udev/libudev or periodic hidapi refresh, isolated in the transport module.
- Persistence: Serde JSON, atomic write.
- Logs: `tracing`, without including macro content by default.
- Initial packaging: Flatpak and native package; AppImage may be additional, but not the only format.

### 7.2 Process

A single process, without a privileged daemon:

```text
GTK/libadwaita UI
       │ commands/events
       ▼
Application Controller
       │
       ├── Domain + validation
       ├── Profile persistence
       └── Dedicated HID worker thread
                 │
                 ├── discovery/open/close
                 ├── protocol encoder/decoder
                 └── hidapi/hidraw
```

The GTK thread must never perform HID I/O. The worker receives commands via a channel and returns typed events.

### 7.3 Modules

```text
src/
  main.rs
  app.rs
  domain/
    action.rs
    config.rs
    geometry.rs
  device/
    discovery.rs
    session.rs
    worker.rs
  protocol/
    frame.rs
    identify.rs
    read_config.rs
    write_config.rs
    codec.rs
  profile/
    json.rs
    migration.rs
  ui/
    device_page.rs
    keyboard_editor.rs
    action_editor.rs
    status.rs
```

Start with a single crate. Split into separate crates only if the protocol starts to be consumed by a CLI or another application.

### 7.4 Session States

```text
Disconnected
  -> Opening
  -> Identifying
  -> ReadyClean | ReadyDirty | ReadOnlyUnknown
  -> Reading | Writing
  -> ReadyClean
  -> ErrorRecoverable | Disconnected
```

Invariants:

- only `ReadyDirty` allows writing;
- `Writing` blocks editing and new reads;
- disconnection immediately invalidates the handle;
- reconnection with a different serial does not reuse the dirty state;
- an unknown device never enters `ReadyDirty` by default.

## 8. User Interface

### 8.1 Main Screen

- device selector with VID:PID, serial, and status;
- data-driven geometry drawing;
- tabs or segmented control for the three layers;
- visual indication of modified positions;
- side action editor;
- actions: `Read from device`, `Write`, `Revert changes`, `Clear key`, `Clear layer`;
- persistent status of the last operation and actionable error.
### 8.2 Action editor

Categories:

- keyboard;
- combination/macro;
- multimedia;
- mouse;
- lighting;
- delay.

The UI must show the firmware's real limit before accepting a sequence. Fields not supported by the variant remain disabled, not silently discarded.

### 8.3 Profiles

- export JSON without private data beyond the optional serial number;
- import with schema and variant validation;
- allow importing a profile from another unit of the same variant after a warning;
- never write automatically after importing;
- keep an automatic backup of the last confirmed reading.

## 9. Functional requirements

| ID | Requirement |
|---|---|
| FR-001 | Discover only devices from the allowlist and select interface 0. |
| FR-002 | Display connection/disconnection without blocking the UI. |
| FR-003 | Query and map the variant tuple. |
| FR-004 | Read the three layers and preserve unknown bytes. |
| FR-005 | Edit all action types from section 4.3. |
| FR-006 | Mark dirty by position and write only changed records. |
| FR-007 | Send commit exactly once per transaction. |
| FR-008 | Re-read and verify after writing. |
| FR-009 | Export/import versioned profiles. |
| FR-010 | Operate without root via udev/uaccess. |
| FR-011 | Offer a read-only diagnostic mode for an unknown variant. |
| FR-012 | Produce a diagnostic report without sensitive macro content. |

## 10. Non-functional requirements

| ID | Requirement |
|---|---|
| NFR-001 | No Windows binary or Wine in the final artifact. |
| NFR-002 | Startup without a device in under 2s on a common machine. |
| NFR-003 | Responsive UI during reading, writing, and hotplug. |
| NFR-004 | No writing without explicit user action. |
| NFR-005 | Atomic and recoverable local profile writing. |
| NFR-006 | Logs without key sequences by default. |
| NFR-007 | Protocol code covered by deterministic binary vectors. |
| NFR-008 | Compatible with Wayland and X11 via GTK. |
| NFR-009 | Initial support for x86_64 and aarch64. |
| NFR-010 | udev rule limited to known VID/PID and `TAG+="uaccess"`. |

## 11. udev permissions

The current `MODE="0666"` rule for every `hidraw*` is insecure and must be removed from the product.

Proposed rule for the validated device:

```udev
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1189", ATTRS{idProduct}=="8842", TAG+="uaccess"
```

Additional PIDs should only be added after validation. Distributions without logind/uaccess may document a dedicated group as a fallback; do not use `0666`.

## 12. Error handling

- **Permission denied:** indicate the exact udev rule and how to reload it.
- **Device busy:** show the likely process when detectable; do not attempt to detach the keyboard interface.
- **Truncated response:** cancel the operation and preserve the previous state.
- **Disconnection during read:** discard the incomplete read.
- **Disconnection during write:** mark the result as indeterminate and require a re-read after reconnection.
- **Timeout after commit:** do not retry the commit; require verification.
- **Unknown tuple:** read-only and diagnostic export.
- **Incompatible profile:** block writing until explicit conversion.

## 13. Hardware validation plan

### Phase A — passive, no writing

1. Confirm VID/PID, serial, interfaces, endpoints, and descriptors.
2. Confirm that discovery selects only the vendor-defined interface.
3. Test repeated connection, disconnection, and reconnection.
4. Compare the descriptor read by the hidapi backend with sysfs.

Criterion: no change in key behavior.

### Phase B — read-only commands

1. Capture the `03 FB FB FB` query via usbmon/Wireshark.
2. Record the full response and the variant tuple.
3. Capture `Read Configuration` in the original application with USBPcap or a controlled environment.
4. Determine exactly the layer/range/block fields.
5. Read the configuration with the Linux implementation and compare byte by byte.

Criterion: three consecutive identical reads with the device idle.

### Phase C — minimal, reversible write

1. Save a complete binary snapshot.
2. Choose a non-critical key on an inactive layer.
3. Change it to a basic simple key.
4. Write a single record and commit.
5. Re-read and confirm only the expected record.
6. Physically test the key.
7. Restore the snapshot and verify again.

Criterion: round-trip with no side effects.

### Phase D — action matrix

Cover at least one case of:

- simple key;
- each modifier;
- combination with multiple modifiers;
- sequence at the limit and above the limit;
- minimum, typical, and maximum delay;
- each multimedia action;
- each mouse action;
- each RGB mode and color;
- three layers;
- key and layer clearing.

Criterion: written bytes match the reference vectors and correct physical behavior.

### Phase E — failures

- remove the cable during reading;
- remove during writing before commit;
- remove immediately after commit;
- deny permission to hidraw;
- connect two units;
- connect an unrelated HID device;
- short response or corrupted capture file in the simulator.

Criterion: no writing to the wrong device, no hangs, and an explicit final state.

### Phase F — distribution

Validate on:

- Arch Linux;
- Fedora;
- Ubuntu LTS;
- GNOME/Wayland;
- KDE/Wayland;
- X11 session;
- x86_64;
- aarch64 when hardware is available.

## 14. Testing strategy

### 14.1 Unit tests

- encode/decode of each frame;
- size and Report ID validation;
- round-trip of records with opaque bytes;
- profile migrations;
- correct interface selection;
- session state machine.

### 14.2 Golden vectors

Keep binary files produced by capture, with separate metadata:

```text
tests/vectors/<variant>/<operation>/
  request.bin
  response.bin
  expected.json
```

The vectors must not contain manufacturer code. Each vector records hardware, firmware, human operation, hash, and capture origin.

### 14.3 Simulator

Implement `MockTransport` that reproduces:
- valid responses;
- timeout;
- short reads;
- disconnection;
- error before/after commit.

The simulator does not replace physical validation.

## 15. Acceptance criteria for the first version

Version 1.0 is ready when:

1. it runs as a native Linux binary, without Wine;
2. it detects `1189:8842` and opens only interface 0;
3. it correctly identifies the validated hardware;
4. it reads and displays the three layers without losing unknown bytes;
5. it edits and writes all categories in section 4.3;
6. it writes only dirty positions, sends the commit, and verifies via re-read;
7. it exports/imports a JSON profile;
8. it works with a restricted udev rule and without root;
9. it passes the hardware matrix in section 13;
10. it contains no code, binaries, or resources from the original application.

## 16. Open questions before full implementation

1. Confirm via capture whether the read transports 64 or 65 bytes on the wire.
2. Name the exact semantics of bytes 2–4 of the `0xFA` command.
3. Finalize all offsets and boundaries of the 50-byte record.
4. Map the complete table of keyboard, consumer, mouse, and RGB codes.
5. Confirm the maximum number of macro events per variant.
6. Validate each alternative PID and its geometry.
7. Determine whether there is an explicit write ACK or only verification via re-read.
8. Confirm whether the commit is global, per-layer, or per-batch.
9. Identify the firmware version and protocol differences between variants.

None of these questions authorizes a trial-and-error write fallback. Any functionality that has not been confirmed must remain disabled or read-only.