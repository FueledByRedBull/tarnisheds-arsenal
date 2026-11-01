import { Info } from "lucide-react";
import type { ReactNode } from "react";
import { Popover } from "./Popover";

// A workspace's rules, one click away instead of a paragraph above every result.
export function AboutPopover({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Popover
      label={label}
      trigger={<><Info size={13} aria-hidden="true" />How it works</>}
      triggerLabel={`How it works: ${label}`}
      triggerClassName="about-trigger"
      panelClassName="editor-panel about-panel"
    >
      <div className="editor-body editor-note">{children}</div>
    </Popover>
  );
}
