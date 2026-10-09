// Opening actions from anywhere: the command menu's and forms' state, kept
// by the app, and a hook for buttons and menus to open an action with what
// they're about already filled in.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'

import { actionById } from '@/lib/actions/catalog'
import { type Filled, givenFrom } from '@/lib/actions/flow'
import type { Action, Prefill } from '@/lib/actions/types'

// How long a finished action's message stays on screen.
const MESSAGE_MS = 4000

type ActionsApi = {
  openMenu: () => void
  runAction: (id: string, prefill?: Prefill) => void
}

const ActionsContext = createContext<ActionsApi>({ openMenu: () => undefined, runAction: () => undefined })

export const ActionsProvider = ActionsContext

export const useActions = () => useContext(ActionsContext)

type View = { kind: 'menu' } | { kind: 'action'; action: Action; given: readonly Filled[]; opening: number }

// The menu's and forms' state, kept by the app so it can open actions too.
export const useActionState = () => {
  const [view, setView] = useState<View>()
  const [message, setMessage] = useState<string>()
  const openings = useRef(0)

  const close = useCallback(() => setView(undefined), [])
  const openMenu = useCallback(() => setView({ kind: 'menu' }), [])
  const start = useCallback((action: Action, given: readonly Filled[]) => {
    openings.current += 1
    setView({ kind: 'action', action, given, opening: openings.current })
  }, [])
  const runAction = useCallback(
    (id: string, prefill: Prefill = {}) => {
      const action = actionById(id)
      if (action !== undefined) start(action, givenFrom(prefill))
    },
    [start],
  )

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== 'k' || !(event.metaKey || event.ctrlKey)) return
      event.preventDefault()
      setView(current => (current === undefined ? { kind: 'menu' } : undefined))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  useEffect(() => {
    if (message === undefined) return
    const timer = window.setTimeout(() => setMessage(undefined), MESSAGE_MS)
    return () => window.clearTimeout(timer)
  }, [message])

  const api = useMemo<ActionsApi>(() => ({ openMenu, runAction }), [openMenu, runAction])
  return { api, view, message, setMessage, close, start }
}

export type ActionState = ReturnType<typeof useActionState>
