import "../shared/build-tools.css";
import { useEffect, useId, useRef, useState } from "react";
import { downloadText } from "../../lib/csv";
import { errorMessage } from "../../lib/format";
import { whenIdle } from "../../lib/idle";
import {
  inspectSavedBuilds, MAX_BUILD_BACKUP_BYTES, previewBuildBackup, recoverSavedBuilds,
  restoreBuildBackup, savedBuildBackupText, type SavedBuildInspection,
} from "../../lib/presets";

function readInspection(): { inspection: SavedBuildInspection | null; error: string } {
  try { return { inspection: inspectSavedBuilds(), error: "" }; }
  catch (caught) { return { inspection: null, error: errorMessage(caught) }; }
}

// The library revision no scan has read yet.
const UNSCANNED = Symbol("unscanned");

export function SavedBuildRecovery({ onChanged, revision }: { onChanged: () => void; revision?: unknown }) {
  const [scanned, setScan] = useState<{ revision: unknown } & ReturnType<typeof readInspection>>(
    { revision: UNSCANNED, inspection: null, error: "" });
  // A scan reads every stored record, so it waits for idle time instead of running inside the
  // render that opened the library or refreshed it after a save.
  useEffect(() => whenIdle(() => setScan({ revision, ...readInspection() }), 0), [revision]);
  // Until the library's current revision is scanned, an earlier scan describes storage as it was.
  const checking = scanned.revision !== revision;
  const inspection = checking ? null : scanned.inspection;
  const { error } = scanned;
  const setError = (next: string) => setScan((current) => ({ ...current, error: next }));
  const reasonId = useId();
  const scanReason = checking ? "Available when the check of saved builds finishes."
    : inspection ? "" : "Available once saved builds can be read. Scan saved builds to try again.";
  const [message, setMessage] = useState("");
  const [backup, setBackup] = useState("");
  const [preview, setPreview] = useState<ReturnType<typeof previewBuildBackup> | null>(null);
  // Restoring writes beside the library, so it waits for the library to be read and listed.
  const restoreReason = scanReason || (inspection?.needsRecovery ? "Recover the saved-build list above before restoring a backup." : "");
  const fileRead = useRef(0);
  useEffect(() => () => { fileRead.current += 1; }, []);

  function scan() {
    setScan({ revision, ...readInspection() });
  }

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
      downloadText("tarnisheds-arsenal-builds.json", text, "application/json;charset=utf-8");
      setScan({ revision, inspection: current, error: "" });
      setMessage(`Exported ${current.presets.length} readable builds.${current.issues.length ? " Review the listed storage issues; unreadable records are excluded." : ""}`);
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
    <summary>Backup and recovery{inspection?.needsRecovery ? ": attention needed" : ""}</summary>
    <p>Back up every readable build across profiles. Restoring keeps existing builds and gives each imported build a new ID.</p>
    {checking ? <p role="status">Checking saved builds...</p> : null}
    {inspection ? <>
      <p>{inspection.presets.length} readable builds · {inspection.orphanCount} unlisted builds · {inspection.issues.length} storage issues</p>
      {inspection.issues.length > 0 ? <ul className="tone-warning">{inspection.issues.map((issue, index) => <li key={index}>{issue}</li>)}</ul> : null}
      {inspection.needsRecovery ? <>
        <p>Recovery will rebuild the list from these {inspection.presets.length} readable builds. It preserves the original index and leaves unreadable records untouched.</p>
        <button type="button" onClick={recover}>Recover {inspection.presets.length} builds</button>
      </> : null}
    </> : null}
    <div className="inspector-actions stacked">
      <button type="button" onClick={scan}>Scan saved builds</button>
      <button type="button" onClick={exportAll} disabled={!inspection} aria-describedby={scanReason ? `${reasonId}-export` : undefined}>Export all builds</button>
      {scanReason ? <small id={`${reasonId}-export`}>{scanReason}</small> : null}
    </div>
    <label>Restore build backup (JSON, up to 10 MiB)
      <input type="file" accept="application/json,.json" onChange={(event) => { void selectBackup(event.target.files?.[0]); event.target.value = ""; }} />
    </label>
    {preview ? <div className="import-preview">
      <p>{preview.presets.length} builds · {preview.bytes.toLocaleString()} bytes · existing builds will be kept</p>
      <ul>{preview.presets.map((preset) => <li key={preset.id}>{preset.name} · {preset.profileId} · {preset.dataVersion}</li>)}</ul>
      <button type="button" onClick={restore} disabled={!preview.presets.length || !inspection || inspection.needsRecovery}
        aria-describedby={restoreReason ? `${reasonId}-restore` : undefined}>Restore {preview.presets.length} builds as copies</button>
      {restoreReason ? <small id={`${reasonId}-restore`}>{restoreReason}</small> : null}
    </div> : null}
    {error ? <p className="tone-danger" role="alert">{error}</p> : null}
    {message ? <p role="status">{message}</p> : null}
  </details>;
}
