// The run log, newest last, reloaded whenever anything changes.

import { useEffect, useState } from 'react'

import type { Entry } from '@/generated/Entry'
import { clockTime, entryKindLabel, entryRecipient } from '@/lib/format'
import { askFor } from '@/lib/legion'

const ALL_ENTRIES = { mission: null, position: null, kinds: null, since_ms: null, open_questions: false }

export function LogTab({ runId, changeCount }: { runId: string; changeCount: number }) {
  const [entries, setEntries] = useState<Entry[]>([])
  const [problem, setProblem] = useState<string>()

  useEffect(() => {
    askFor('entries', { type: 'log', run: runId, filter: ALL_ENTRIES })
      .then(reply => {
        setEntries(reply.entries)
        setProblem(undefined)
      })
      .catch((error: unknown) => setProblem(String(error)))
  }, [runId, changeCount])

  if (problem !== undefined) return <p className="text-danger">{problem}</p>
  return (
    <ol className="flex flex-col font-mono">
      {entries.map(entry => (
        <li key={entry.id} className="grid grid-cols-[var(--space-8)_1fr] gap-3 border-b border-border py-1">
          <span className="text-muted-foreground">{clockTime(entry.at_ms)}</span>
          <span>
            <span className="text-muted-foreground">
              {entry.from}
              {entryRecipient(entry)} {entryKindLabel(entry)}
              {entry.mission !== null && ` m${entry.mission}`}:{' '}
            </span>
            <span className="whitespace-pre-wrap font-sans">{entry.text}</span>
          </span>
        </li>
      ))}
    </ol>
  )
}
