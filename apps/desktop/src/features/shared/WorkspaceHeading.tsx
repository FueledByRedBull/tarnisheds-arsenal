import type { ReactNode } from "react";
import { AboutPopover } from "./AboutPopover";

// Every analysis section opens the same way: its title with the rules one click away, then
// one summary line (children) as the entry point.
export function WorkspaceHeading({ title, aboutLabel, about, level = 1, id, children }: {
  title: ReactNode;
  aboutLabel: string;
  about: ReactNode;
  level?: 1 | 2;
  id?: string;
  children?: ReactNode;
}) {
  const Heading = level === 1 ? "h1" : "h2";
  return (
    <div className="workspace-heading-copy">
      <div className="workspace-title-row">
        <Heading id={id}>{title}</Heading>
        <AboutPopover label={aboutLabel}>{about}</AboutPopover>
      </div>
      {children}
    </div>
  );
}
