export type AffinityLineStyle = { color: string; dash: string | undefined };

/** Series strokes are this wide with round caps, which eat that much of every dash gap. */
export const AFFINITY_STROKE_WIDTH = 2.5;
/** The legend swatch is long enough to show one full period of every dash pattern. */
export const AFFINITY_SWATCH_WIDTH = 36;

// Series hues stay clear of the status colours (danger red, warning ember, success green).
// Every line gets its own dash pattern, so lines can be told apart without colour; colours
// repeat after ten lines, and neighbouring lines never share a colour or a dash. Each gap is
// at least 6 units, so a gap of 3.5px stays visible after the round caps.
const DASHES = [
  undefined, "14 6", "6 6", "1 6", "14 6 1 6", "24 7", "1 6 1 12",
  "8 6 1 6 1 6", "14 6 6 6", "1 12", "6 6 6 14", "3 9", "18 6 3 6",
] as const;

const COLORS = [
  "#e0b967",
  "#79bfd0",
  "#b59ad8",
  "#d7d2b6",
  "#729fe0",
  "#d889b3",
  "#b7cb72",
  "#8f86d8",
  "#a7c7e7",
  "#c9a0dc",
];

export function affinityLineStyle(index: number): AffinityLineStyle {
  return { color: COLORS[index % COLORS.length], dash: DASHES[index % DASHES.length] };
}
