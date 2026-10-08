// The main area's open tabs, remembered on this machine so a reload comes
// back to the same pages.

import { useCallback, useEffect, useState } from 'react'

import { closeTab, NO_TABS, openTab, type OpenTabs, parseOpenTabs, type Tab } from '@/lib/tabs'

const STORAGE_KEY = 'legion.openTabs'

// Storage can be missing or refuse; the app starts with no tabs then.
const readOpenTabs = (): OpenTabs => {
  try {
    return parseOpenTabs(window.localStorage.getItem(STORAGE_KEY))
  } catch {
    return NO_TABS
  }
}

const saveOpenTabs = (open: OpenTabs) => {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(open))
  } catch {
    // Not remembered; the tabs stay open until the page reloads.
  }
}

export const useOpenTabs = () => {
  const [open, setOpen] = useState(readOpenTabs)
  useEffect(() => saveOpenTabs(open), [open])
  const show = useCallback((tab: Tab) => setOpen(current => openTab(current, tab)), [])
  const close = useCallback((key: string) => setOpen(current => closeTab(current, key)), [])
  const activate = useCallback((key: string) => setOpen(current => ({ ...current, activeKey: key })), [])
  return { open, show, close, activate }
}
