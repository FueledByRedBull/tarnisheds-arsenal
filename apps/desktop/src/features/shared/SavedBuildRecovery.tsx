import { useEffect, useRef, useState } from "react";
import {
  inspectSavedBuilds, MAX_BUILD_BACKUP_BYTES, previewBuildBackup, recoverSavedBuilds,
  restoreBuildBackup, savedBuildBackupText, type SavedBuildInspection,
} from "../../lib/presets";

export function SavedBuildRecovery({ onChanged, revision }: { onChanged: () => void; revision?: unknown }) {
  const [inspection, setInspection] = useState<SavedBuildInspection | null>(null);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [backup, setBackup] = useState("");
  const [preview, setPreview] = useState<ReturnType<typeof previewBuildBackup> | null>(null);
  const fileRead = useRef(0);
  useEffect(() => () => { fileRead.current += 1; }, []);

  function scan() {
    try { setInspection(inspectSavedBuilds()); setError(""); }
    catch (caught) { setInspection(null); setError(errorMessage(caught)); }
  }
  useEffect(scan, [revision]);

  function recover() {
    if (!inspection) return;
    try {
      const preserved = recoverSavedBuilds(inspection);
      setMessage(`Recovered ${inspection.presets.length} builds.${preserved ? " Original index preserved on this device." : ""} Unreadable records were left untouched.`);
      scan();
      onChanged();
    } catch (caught) { setError(errorMessage(caught)); }
  }

  function exportAll() {
    try {
      const current = inspectSavedBuilds();
      const text = savedBuildBackupText(current);
      const url = URL.createObjectURL(new Blob([text], { type: "application/json;charset=utf-8" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = "tarnisheds-arsenal-builds.json";
      link.click();
      URL.revokeObjectURL(url);
      setInspection(current);
      setMessage(`Exported ${current.presets.length} readable builds.${current.issues.length ? " Review the listed storage issues; unreadable records are excluded." : ""}`);
      setError("");
    } catch (caught) { setError(errorMessage(caught)); }
  }

  async function selectBackup(file: File | undefined) {
    const read = ++fileRead.current;
    setPreview(null);
    setBackup("");
    setMessage("");
    if (!file) return;
    try {
      if (file.size > MAX_BUILD_BACKUP_BYTES) throw new Error("Build backup is too large (limit 10 MiB).");
      const text = await file.text();
      if (read !== fileRead.current) return;
      setPreview(previewBuildBackup(text));
      setBackup(text);
      setError("");
    } catch (caught) { if (read === fileRead.current) setError(errorMessage(caught)); }
  }

  function restore() {
    try {
      const restored = restoreBuildBackup(backup);
      setMessage(`Restored ${restored.length} builds as new copies. Existing builds were preserved; load them through Saved Builds to check their profile and data version.`);
      setPreview(null);
      setBackup("");
      scan();
      onChanged();
    } catch (caught) { setError(errorMessage(caught)); }
  }

  return <details className="saved-build-recovery" open={inspection?.needsRecovery || undefined}>
    <summary>Backup and recovery{inspection?.needsRecovery ? " — attention needed" : ""}</summary>
    <p>Back up every readable build across profiles. Restoring keeps existing builds and gives each imported build a new ID.</p>
    {inspection ? <>
      <p>{inspection.presets.length} readable builds · {inspection.orphanCount} unlisted builds · {inspection.issues.length} storage issues</p>
      {inspection.issues.length > 0 ? <ul className="warning-text">{inspection.issues.map((issue, index) => <li key={index}>{issue}</li>)}</ul> : null}
      {inspection.needsRecovery ? <>
        <p>Recovery will rebuild the list from these {inspection.presets.length} readable builds. It preserves the original index and leaves unreadable records untouched.</p>
        <button type="button" onClick={recover}>Recover {inspection.presets.length} builds</button>
      </> : null}
    </> : null}
    <div className="inspector-actions stacked">
      <button type="button" onClick={scan}>Scan saved builds</button>
      <button type="button" onClick={exportAll} disabled={!inspection}>Export all builds</button>
    </div>
    <label>Restore build backup (JSON, up to 10 MiB)
      <input type="file" accept="application/json,.json" onChange={(event) => { void selectBackup(event.target.files?.[0]); event.target.value = ""; }} />
    </label>
    {preview ? <div className="import-preview">
      <p>{preview.presets.length} builds · {preview.bytes.toLocaleString()} bytes · existing builds will be kept</p>
      <ul>{preview.presets.map((preset) => <li key={preset.id}>{preset.name} · {preset.profileId} · {preset.dataVersion}</li>)}</ul>
      <button type="button" onClick={restore} disabled={!preview.presets.length || inspection?.needsRecovery}>Restore {preview.presets.length} builds as copies</button>
    </div> : null}
    {error ? <p className="warning-text" role="alert">{error}</p> : null}
    {message ? <p role="status">{message}</p> : null}
  </details>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
