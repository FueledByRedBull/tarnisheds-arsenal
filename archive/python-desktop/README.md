# Archived Python Desktop

This directory keeps the retired PyQt desktop UI and its release/smoke helpers for reference while the active desktop app lives in `apps/desktop`.

The archived code is not part of the supported release path. Current Windows releases are built with Tauri through:

```powershell
python tools/phase4/package_release.py
```

Keep this archive for behavior comparisons only. New UI work should target the TypeScript/Tauri app.
