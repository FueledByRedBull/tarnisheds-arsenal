import { ReactNode, useCallback, useEffect, useId, useRef } from "react";
import { flashChanged } from "../../lib/motion";

const GAP = 6;
const EDGE = 8;

// A native popover anchored under its trigger. The browser supplies light dismiss, Escape,
// top-layer rendering and focus return; this only places the panel and keeps it on screen.
// Closed panels stay mounted (display: none), so effects inside them keep running.
export function Popover({
  label,
  trigger,
  triggerLabel,
  triggerClassName,
  triggerTitle,
  disabled = false,
  panelClassName,
  changeKey,
  children,
}: {
  label: string;
  trigger: ReactNode;
  triggerLabel?: string;
  triggerClassName?: string;
  triggerTitle?: string;
  disabled?: boolean;
  panelClassName?: string;
  /** The trigger flashes when this changes while its editor is closed (undo, palette, …). */
  changeKey?: string;
  children: ReactNode | ((close: () => void) => ReactNode);
}) {
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const close = useCallback(() => {
    if (panel.current?.matches(":popover-open")) panel.current.hidePopover();
  }, []);

  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    // Anchor before the panel paints, then keep it inside the window once its size is known.
    const anchorBelow = () => {
      const anchor = button.current?.getBoundingClientRect();
      if (!anchor) return null;
      element.style.left = `${Math.max(EDGE, anchor.left)}px`;
      element.style.top = `${anchor.bottom + GAP}px`;
      return anchor;
    };
    const place = () => {
      const anchor = anchorBelow();
      if (!anchor) return;
      const box = element.getBoundingClientRect();
      if (box.right > window.innerWidth - EDGE) {
        element.style.left = `${Math.max(EDGE, window.innerWidth - EDGE - box.width)}px`;
      }
      if (box.bottom > window.innerHeight - EDGE && anchor.top - GAP - box.height >= EDGE) {
        element.style.top = `${anchor.top - GAP - box.height}px`;
      }
    };
    const onBeforeToggle = (event: Event) => {
      if ((event as ToggleEvent).newState === "open") anchorBelow();
    };
    const onToggle = (event: Event) => {
      if ((event as ToggleEvent).newState !== "open") return;
      place();
      if (!element.contains(document.activeElement)) element.focus({ preventScroll: true });
    };
    const onResize = () => {
      if (element.matches(":popover-open")) place();
    };
    element.addEventListener("beforetoggle", onBeforeToggle);
    element.addEventListener("toggle", onToggle);
    window.addEventListener("resize", onResize);
    return () => {
      element.removeEventListener("beforetoggle", onBeforeToggle);
      element.removeEventListener("toggle", onToggle);
      window.removeEventListener("resize", onResize);
    };
  }, []);

  useEffect(() => {
    if (disabled) close();
  }, [close, disabled]);

  const shownKey = useRef(changeKey);
  useEffect(() => {
    if (shownKey.current === changeKey) return;
    shownKey.current = changeKey;
    if (!panel.current?.matches(":popover-open")) flashChanged(button.current);
  }, [changeKey]);

  return (
    <>
      <button
        ref={button}
        type="button"
        className={triggerClassName}
        popoverTarget={id}
        aria-haspopup="dialog"
        aria-label={triggerLabel}
        title={triggerTitle}
        disabled={disabled}
      >
        {trigger}
      </button>
      <div
        ref={panel}
        id={id}
        popover="auto"
        role="dialog"
        aria-label={label}
        tabIndex={-1}
        className={`popover-panel${panelClassName ? ` ${panelClassName}` : ""}`}
      >
        {typeof children === "function" ? children(close) : children}
      </div>
    </>
  );
}
