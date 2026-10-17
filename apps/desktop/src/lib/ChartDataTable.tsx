import { useState } from "react";

// A chart's values as a table behind a disclosure: long series add no DOM or accessibility
// tree rows until someone opens it.
export function ChartDataTable({ caption, headers, rows }: { caption: string; headers: string[]; rows: string[][] }) {
  const [open, setOpen] = useState(false);
  // Cleared results drop the <details>, so the next run's disclosure must start closed too.
  if (!rows.length && open) setOpen(false);
  if (!rows.length) return null;
  return (
    <details className="chart-data" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>Data table</summary>
      {open ? (
        <table>
          <caption>{caption}</caption>
          <thead><tr>{headers.map((header) => <th key={header}>{header}</th>)}</tr></thead>
          <tbody>{rows.map((row, index) => <tr key={index}>{row.map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody>
        </table>
      ) : null}
    </details>
  );
}
