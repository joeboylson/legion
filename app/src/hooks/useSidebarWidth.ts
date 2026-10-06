// The sidebar's width, dragged by its edge and remembered on this machine.

import { type PointerEvent as ReactPointerEvent, useCallback, useState } from 'react'

import { clampSidebarWidth, DEFAULT_SIDEBAR_WIDTH_PX, parseSavedWidth } from '@/lib/sidebar-width'

const STORAGE_KEY = 'legion.sidebarWidth'

// Storage can be missing or refuse; the width just isn't remembered then.
const readSavedWidth = (): number => {
  try {
    return parseSavedWidth(window.localStorage.getItem(STORAGE_KEY))
  } catch {
    return DEFAULT_SIDEBAR_WIDTH_PX
  }
}

const saveWidth = (widthPx: number) => {
  try {
    window.localStorage.setItem(STORAGE_KEY, String(widthPx))
  } catch {
    // Not remembered; it still applies until the app closes.
  }
}

export const useSidebarWidth = () => {
  const [widthPx, setWidthPx] = useState(readSavedWidth)

  const startResize = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthPx
    const follow = (move: PointerEvent) => setWidthPx(clampSidebarWidth(startWidth + move.clientX - startX))
    const stop = (up: PointerEvent) => {
      saveWidth(clampSidebarWidth(startWidth + up.clientX - startX))
      window.removeEventListener('pointermove', follow)
      window.removeEventListener('pointerup', stop)
    }
    window.addEventListener('pointermove', follow)
    window.addEventListener('pointerup', stop)
  }, [widthPx])

  return { widthPx, startResize }
}
