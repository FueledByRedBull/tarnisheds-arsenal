export type AffinityLineStyle = { color: string; dash: string | undefined };

// Series hues stay clear of the status colours (danger red, warning ember, success green).
// Every line gets its own dash pattern, so lines can be told apart without colour; colours
// repeat after ten lines, and neighbouring lines never share a colour or a dash.
const DASHES = [
  undefined, "12 6", "3 5", "14 4 3 4", "6 6", "18 5", "2 4",
  "10 4 2 4 2 4", "4 2", "16 4 6 4", "8 3 2 3", "20 4 2 4", "1 3",
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
