// Which sidebar rows are open, remembered on this machine so a reload keeps
// the tree as it was.

import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'

import { parseOpenRows, withRowOpen } from '@/lib/open-rows'

const STORAGE_KEY = 'legion.openSidebarRows'

// Storage can be missing or refuse; rows just start closed then.
const readOpenRows = (): ReadonlySet<string> => {
  try {
    return parseOpenRows(window.localStorage.getItem(STORAGE_KEY))
  } catch {
    return new Set()
  }
}

const saveOpenRows = (openRows: ReadonlySet<string>) => {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify([...openRows]))
  } catch {
    // Not remembered; the rows stay as they are until the page reloads.
  }
}

type OpenRows = { isOpen: (key: string) => boolean; setOpen: (key: string, isOpen: boolean) => void }

export const useOpenRowsState = (): OpenRows => {
  const [openRows, setOpenRows] = useState(readOpenRows)
  useEffect(() => saveOpenRows(openRows), [openRows])
  const setOpen = useCallback((key: string, isOpen: boolean) => setOpenRows(rows => withRowOpen(rows, key, isOpen)), [])
  const isOpen = useCallback((key: string) => openRows.has(key), [openRows])
  return useMemo(() => ({ isOpen, setOpen }), [isOpen, setOpen])
}

export const OpenRowsContext = createContext<OpenRows | undefined>(undefined)

// The props that make a Collapsible remember whether it's open.
export const useRowOpen = (key: string) => {
  const openRows = useContext(OpenRowsContext)
  if (openRows === undefined) throw new Error('useRowOpen needs an OpenRowsContext above it')
  return { open: openRows.isOpen(key), onOpenChange: (isOpen: boolean) => openRows.setOpen(key, isOpen) }
}
