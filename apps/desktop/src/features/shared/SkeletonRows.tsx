import { CSSProperties } from "react";

// Placeholder rows shaped like the ranking they stand in for. Each row's shimmer starts a beat
// after the one above, so it rolls down the table.
export function SkeletonRows({ count }: { count: number }) {
  return (
    <div className="skeleton-rows" aria-hidden="true">
      {Array.from({ length: count }, (_, index) => (
        <div className="skeleton-row" key={index} style={{ "--sweep-delay": `${index * 70}ms` } as CSSProperties}>
          <span className="skeleton" />
          <span><span className="skeleton" /><span className="skeleton" /></span>
          <span className="skeleton" />
          <span className="skeleton" />
          <span className="skeleton" />
        </div>
      ))}
    </div>
  );
}
