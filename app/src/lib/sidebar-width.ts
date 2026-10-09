// The sidebar's width when the admin drags its edge.

// Fibonacci bounds from the design tokens: --space-9 and --measure.
export const MIN_SIDEBAR_WIDTH_PX = 144
export const MAX_SIDEBAR_WIDTH_PX = 610
export const DEFAULT_SIDEBAR_WIDTH_PX = 233

export const clampSidebarWidth = (widthPx: number): number =>
  Math.round(Math.min(MAX_SIDEBAR_WIDTH_PX, Math.max(MIN_SIDEBAR_WIDTH_PX, widthPx)))

// A remembered width, or the default when there's none or it isn't a number.
export const parseSavedWidth = (saved: string | null): number => {
  const width = saved === null ? Number.NaN : Number.parseFloat(saved)
  return Number.isFinite(width) ? clampSidebarWidth(width) : DEFAULT_SIDEBAR_WIDTH_PX
}
