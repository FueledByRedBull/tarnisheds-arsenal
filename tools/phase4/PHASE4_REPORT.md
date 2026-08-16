# Phase 4 Report

## Scope Completed
- Data/math regression validation
- TypeScript/Tauri build validation
- Runtime command validation
- Scaling-aware optimizer search-space validation
- Tauri release packaging with frozen data snapshot
- Installer/portable executable instructions in release bundle

## Validation Commands

```powershell
cargo test --manifest-path core/er_optimizer_core/Cargo.toml
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
python tools/phase4/benchmark_optimizer_phases.py --warmups 1 --repeats 5
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

- `dist/TarnishedsArsenal_<version>`
- `dist/TarnishedsArsenal_<version>.zip`

Release-directory contents:
- Windows MSI installer
- Self-contained standalone executable
- SHA-256 checksums
- Machine-readable build report with commit, data snapshot, artifact hashes, and gates
- `LICENSE`
- `README.md`

The ZIP is a versioned archive of that complete release directory.

## Clean Install/Run Check

```powershell
python tools/phase4/package_release.py
Start-Process dist/TarnishedsArsenal_<version>/TarnishedsArsenal_<version>_portable.exe
```

Release builds load one complete compile-time snapshot. The MSI and standalone
executable do not require external runtime data.

## Optimizer Search Model

The optimizer now counts and visits stat candidates per weapon, affinity, and
Ash of War using only stats that can affect the selected objective. Weapon
requirements are applied before enumeration, including two-hand STR reduction,
and inactive stats are filled deterministically only when needed to consume the
remaining level budget. Search estimates therefore reflect the reduced exact
candidate space, especially for broad fixed-upgrade/all-affinity runs.
