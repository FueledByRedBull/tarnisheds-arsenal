import { useState } from "react";
import { reproductionReport } from "../../lib/reproduction-report";
import { useDesktopStore } from "../../lib/state";

export function ReproductionReport() {
  const [preview, setPreview] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  function capture() {
    try {
      setPreview(reproductionReport(useDesktopStore.getState()));
      setError(null);
    } catch {
      setPreview(null);
      setError("The current inputs could not be captured. Check the inputs and try again.");
    }
  }
  function download() {
    if (!preview) return;
    let url: string | undefined;
    try {
      url = URL.createObjectURL(new Blob([preview], { type: "application/json" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = "tarnisheds-arsenal-reproduction.json";
      link.click();
      setError(null);
    } catch {
      setError("The report could not be downloaded. You can copy the preview text instead.");
    } finally {
      if (url) URL.revokeObjectURL(url);
    }
  }
  return <div className="reproduction-report">
    <button className="clear-locks" type="button" onClick={capture}>Preview reproduction report</button>
    {error ? <p role="alert">{error}</p> : null}
    {preview ? <div className="report-preview">
      <p>Review before sharing. This captures current inputs and the displayed selection when it is not stale; it does not recompute saved results. Your saved-build library and raw logs are excluded; text that looks like a personal path or credential is omitted. Nothing is sent automatically.</p>
      <label>Reproduction report preview<textarea readOnly rows={10} value={preview} /></label>
      <div className="inspector-actions">
        <button type="button" onClick={download}>Download report</button>
        <button type="button" onClick={() => setPreview(null)}>Close report</button>
      </div>
    </div> : null}
  </div>;
}
