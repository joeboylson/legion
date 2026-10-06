import { describe, expect, it } from 'vitest'

import { clampSidebarWidth, DEFAULT_SIDEBAR_WIDTH_PX, MAX_SIDEBAR_WIDTH_PX, MIN_SIDEBAR_WIDTH_PX, parseSavedWidth } from './sidebar-width'

describe('sidebar width', () => {
  it('stays within its bounds', () => {
    expect(clampSidebarWidth(10)).toBe(MIN_SIDEBAR_WIDTH_PX)
    expect(clampSidebarWidth(5000)).toBe(MAX_SIDEBAR_WIDTH_PX)
    expect(clampSidebarWidth(300.4)).toBe(300)
  })

  it('reads a saved width, or falls back to the default', () => {
    expect(parseSavedWidth('377')).toBe(377)
    expect(parseSavedWidth('9999')).toBe(MAX_SIDEBAR_WIDTH_PX)
    expect(parseSavedWidth(null)).toBe(DEFAULT_SIDEBAR_WIDTH_PX)
    expect(parseSavedWidth('wide')).toBe(DEFAULT_SIDEBAR_WIDTH_PX)
  })
})
