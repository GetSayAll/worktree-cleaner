// The icon set, as path data on a 16×16 grid.
//
// Hand-drawn rather than pulled from a pack: the app needs nine marks and three
// supporting glyphs, and a dependency would bring a hundred more. Every stroke is
// 1.5 at 16px so the set reads as one hand, and the shapes stay simple enough to
// hold their silhouette at 16 and 18 pixels.

export const ICONS = {
  home: ["M2.5 7 8 2.5 13.5 7v6.5H9.25V9.5h-2.5v4H2.5z"],
  overview: [
    "M2.5 2.5h4.25v4.25H2.5z",
    "M9.25 2.5h4.25v4.25H9.25z",
    "M2.5 9.25h4.25v4.25H2.5z",
    "M9.25 9.25h4.25v4.25H9.25z"
  ],
  // The branch mark, matching the glyph in the app icon (brand.svg). The three
  // endpoints are rings so the mark belongs to the same stroked icon set as its
  // neighbours; the app icon fills the same points at large sizes.
  branch: [
    "M5.5 4v8",
    "M5.5 6.75 10.75 4.75",
    "M5.5 9.25 11.25 11.25",
    "M4.5 4a1 1 0 1 0 2 0a1 1 0 1 0 -2 0",
    "M9.75 4.75a1 1 0 1 0 2 0a1 1 0 1 0 -2 0",
    "M10.25 11.25a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"
  ],
  artifacts: ["M2.75 5.25 8 2.75l5.25 2.5v5.5L8 13.25l-5.25-2.5z", "M2.75 5.25 8 7.75l5.25-2.5", "M8 7.75v5.5"],
  hygiene: [
    "M8 2.25 13 4.4v3.7c0 2.85-2.05 4.7-5 5.65-2.95-.95-5-2.8-5-5.65V4.4z",
    "M5.9 7.9 7.35 9.35 10.2 6.5"
  ],
  plan: [
    "M6 4.25h7.25",
    "M6 8h7.25",
    "M6 11.75h7.25",
    "M2.4 4.2 3.25 5.05 4.7 3.35",
    "M2.4 7.95 3.25 8.8 4.7 7.1",
    "M2.4 11.7 3.25 12.55 4.7 10.85"
  ],
  result: ["M14 8a6 6 0 1 1-12 0 6 6 0 0 1 12 0z", "M5.5 8.2 7.2 9.9 10.5 6.4"],
  history: ["M14 8a6 6 0 1 1-12 0 6 6 0 0 1 12 0z", "M8 4.75V8l2.25 1.35"],
  settings: [
    "M2.75 4.5h6.5",
    "M12.25 4.5h1",
    "M11.25 4.5a1.25 1.25 0 1 0-2.5 0 1.25 1.25 0 0 0 2.5 0z",
    "M2.75 8h1.5",
    "M8 8h5.25",
    "M7.75 8a1.25 1.25 0 1 0-2.5 0 1.25 1.25 0 0 0 2.5 0z",
    "M2.75 11.5h6.5",
    "M12.25 11.5h1",
    "M11.25 11.5a1.25 1.25 0 1 0-2.5 0 1.25 1.25 0 0 0 2.5 0z"
  ],
  folder: ["M2.25 4.5h4l1.2 1.5h6.3v6.25H2.25z"],
  lock: ["M4.25 7V5.4a3.75 3.75 0 0 1 7.5 0V7", "M3.25 7h9.5v6.25h-9.5z", "M8 9.4v1.6"],
  evidence: ["M4 2.5h4.5L12 6v7.5H4z", "M8.5 2.5V6H12", "M6.2 9.2 7.4 10.4 9.9 7.9"],
  trash: [
    "M3 4.5h10",
    "M6 4.5V3h4v1.5",
    "M4.5 4.5l.6 8.5h5.8l.6-8.5",
    "M6.5 7v3.5",
    "M9.5 7v3.5"
  ],
  chevron: ["M6 3.5 10.5 8 6 12.5"],
  search: ["M7 12a5 5 0 1 0 0-10 5 5 0 0 0 0 10z", "M10.6 10.6 14 14"],
  alert: ["M8 2.25 14 13.25H2z", "M8 6v3.25", "M8 11.25v.25"],
  info: ["M14 8a6 6 0 1 1-12 0 6 6 0 0 1 12 0z", "M8 7.25V11", "M8 5v.25"],
  check: ["M3.25 8.4 6.4 11.5 12.8 4.75"],
  close: ["M4 4l8 8", "M12 4l-8 8"],
  dash: ["M4 8h8"]
} as const;

export type IconName = keyof typeof ICONS;
