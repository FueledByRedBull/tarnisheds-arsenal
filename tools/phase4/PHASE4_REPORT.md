# Phase 4 Report

## Scope Completed
- Data/math regression validation
- TypeScript/Tauri build validation
- Runtime command validation
- Tauri release packaging with frozen data snapshot
- Installer/portable executable instructions in release bundle

## Validation Commands

```powershell
cargo test --manifest-path core/er_optimizer_core/Cargo.toml
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
python tools/phase4/validate_phase4.py
cd apps/desktop
npm run build
```

All passed on this workspace.

## Packaging Command

```powershell
python tools/phase4/package_release.py
```

This is now a gated release command: it runs core and Tauri `cargo test`, runs `validate_phase4.py`, installs frontend dependencies with `npm ci`, runs the Tauri production build, and only then writes `dist/`.

Generated:

`dist/TarnishedsArsenal_<version>`

Contents:
- Windows MSI installer
- Portable executable
- `LICENSE`
- `README.md`

## Clean Install/Run Check

```powershell
python tools/phase4/package_release.py
Start-Process dist/TarnishedsArsenal_<version>/tarnisheds-arsenal-desktop.exe
```

The Tauri bundle loads the committed `data/phase1` snapshot as an app resource.

## Note
- `validate_phase4.py` reports a warning that PyQt level-path checks are skipped because the retired UI is archived under `archive/python-desktop`.
