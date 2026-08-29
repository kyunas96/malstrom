export const TAG_COLORS = [
  { fg: '#c98a1f', bg: '#fff8ea' }, // gold — 1st tag
  { fg: '#2f6fb0', bg: '#eef4fa' }, // blue — 2nd tag
  { fg: '#4a8c3f', bg: '#f0f7ee' }, // green — 3rd tag
  { fg: '#a6438c', bg: '#faeef6' }, // magenta — 4th tag
] as const;

export function colorForTagIndex(index: number): (typeof TAG_COLORS)[number] {
  return TAG_COLORS[index % TAG_COLORS.length];
}
