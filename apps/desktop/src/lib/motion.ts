// Motion that explains state: values that changed elsewhere flash, and a ranking re-forges
// in place. Everything runs on the Web Animations API and animates only transform, opacity or
// colour, so it never blocks input or adds layout work beyond one read.

export const EASE_OUT = "cubic-bezier(0.16, 1, 0.3, 1)";

export const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// A brief gold wash on a control whose value changed from somewhere else (the palette, undo,
// a class change). Colour only, so it stays under reduced motion: it is feedback, not travel.
export function flashChanged(element: Element | null): void {
  element?.animate(
    [
      { backgroundColor: "color-mix(in srgb, var(--accent) 30%, transparent)", boxShadow: "0 0 0 1px color-mix(in srgb, var(--accent) 55%, transparent)" },
      { backgroundColor: "transparent", boxShadow: "0 0 0 1px transparent" },
    ],
    { duration: 700, easing: EASE_OUT },
  );
}

// Re-forges a ranking as one layer: the rows group fades after a sort and rises with fresh
// results, then the movement markers in view arrive. The group is always composited
// (styles.css), so starting this costs no layout. Moving rows one by one cost 15-20 ms of style
// and paint per change: each moving row became a layer, and so did its sticky cells.
export function settleRanking(container: HTMLElement, freshResults: boolean, board: HTMLElement | null): void {
  const still = reducedMotion();
  if (still && !freshResults) return;
  container.animate(
    still ? [{ opacity: 0.55 }, { opacity: 1 }]
      : freshResults ? [{ opacity: 0.3, transform: "translateY(6px)" }, { opacity: 1, transform: "none" }]
      : [{ opacity: 0.45 }, { opacity: 1 }],
    { duration: still ? 160 : freshResults ? 280 : 200, easing: still ? "linear" : EASE_OUT },
  );
  if (still || !freshResults) return;
  // Markers start in the next frame and reuse its layout. Reading row offsets now would lay out
  // every new row inside React's commit, joining two long tasks into one.
  requestAnimationFrame(() => {
    // Rows may be shifted by a sort, so compare where they are drawn with the board's view.
    const view = board?.getBoundingClientRect() ?? { top: 0, bottom: window.innerHeight };
    // Opacity only: a moving marker would make every later row its own layer as well.
    [...container.querySelectorAll<HTMLElement>(".rank-move, .metric-delta")]
      .filter((marker) => {
        const row = marker.closest(".result-row-full")?.getBoundingClientRect();
        return row !== undefined && row.bottom > view.top && row.top < view.bottom;
      })
      .forEach((marker, index) => marker.animate(
        [{ opacity: 0 }, { opacity: 1 }],
        { duration: 240, delay: 200 + Math.min(index, 12) * 24, easing: EASE_OUT, fill: "backwards" },
      ));
  });
}
