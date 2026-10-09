// A mission's way through its statuses, from its log: who started on it,
// who handed it to whom, and when it was done and finished.

import type { Entry } from '@/generated/Entry'

export type PathStep = { id: number; atMs: number; text: string }

export const missionStep = (entry: Entry): string | undefined => {
  const who = entry.from
  switch (entry.kind) {
    case 'mission_added':
      return 'added'
    case 'session_started':
      return `${who} started on it`
    case 'handoff': {
      const next = entry.text.startsWith('→ ') ? entry.text.slice(2).split(':')[0] : 'the next step'
      return `${who} handed it to ${next}`
    }
    case 'done':
      return `${who} reported it done`
    case 'blocked':
      return `${who} reported it blocked`
    case 'paused':
      return 'paused'
    case 'resumed':
      return 'resumed'
    case 'finished':
      return 'finished onto the base branch'
    default:
      return undefined
  }
}

export const missionPath = (entries: readonly Entry[]): PathStep[] =>
  entries.flatMap(entry => {
    const text = missionStep(entry)
    return text === undefined ? [] : [{ id: entry.id, atMs: entry.at_ms, text }]
  })
