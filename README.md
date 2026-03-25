<p align="center">
    <img src="./res/icon/logo.png" width="128" alt="ear logo">
    <h1 align=center>ear (native)</h1>
</p>

---

https://github.com/user-attachments/assets/3bc42663-332d-465d-886e-3e60b27c935d

Native Rust desktop client for Nothing and CMF audio devices. The app uses `iced` for the UI and includes platform-specific Bluetooth backends for Linux and Windows.

## Platform Support

- Linux: uses `bluer` with BlueZ/RFCOMM.
- Windows: uses the `windows` crate with WinRT Bluetooth and RFCOMM sockets.

## Requirements

- Rust stable toolchain
- Linux only:
    - BlueZ
    - D-Bus development headers if your distribution packages them separately, for example `libdbus-1-dev` on Debian/Ubuntu
- Windows only:
    - Windows 10 or Windows 11
    - Devices should already be paired before launching the app, since the Windows backend enumerates paired devices

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run
```

## Current Functionality

- Device discovery and connection management
- Automatic model identification from SKU, with name/firmware fallback
- Battery status for left bud, right bud, and case when exposed by the device
- ANC controls with model-specific modes such as off, transparency, low, mid, high, and adaptive
- EQ controls with model-specific preset layouts
- Three-band custom EQ for all mapped models except Nothing Ear (1)
- Advanced EQ toggle on supported models
- Enhanced bass / ultra bass toggle and level control on supported models
- Personalized ANC toggle on supported models
- Ear tip fit test on supported models
- Find-my-earbuds ring controls
- In-ear detection toggle
- Low-latency mode toggle
- Firmware version display
- Reactive async Bluetooth I/O through background tasks

## Supported Models

The Rust model mapping currently includes these product families and color variants:

- Nothing Ear (1)
- Nothing Ear (stick)
- Nothing Ear (2)
- Nothing Ear
- Nothing Ear (a)
- Nothing Ear (open)
- CMF Buds Pro
- CMF Buds
- CMF Buds Pro 2
- CMF Neckband Pro

Capability support is model-dependent. For example, personalized ANC is currently enabled only for Nothing Ear (2), and custom EQ is disabled for Nothing Ear (1).

## Notes On Model-Specific Controls

- EQ preset IDs differ between device families. CMF Buds and CMF Buds Pro 2 use listening-mode commands instead of the standard EQ command path.
- Advanced EQ is supported only on selected models, currently the families mapped to `B155`, `B157`, `B171`, and `B174`.
- Enhanced bass controls are enabled only on selected models, currently `B171`, `B172`, `B168`, and `B162`.
- Ear tip fit test is enabled only on selected models, currently `B155`, `B171`, `B172`, and `B162`.
