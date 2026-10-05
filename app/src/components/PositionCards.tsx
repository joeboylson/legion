// The run's positions as cards. Clicking one opens its live terminal.

import type { SessionInfo } from '@/generated/SessionInfo'
import { ACTIVITY_COLORS, ACTIVITY_LABELS } from '@/lib/format'
import { cn } from '@/lib/utils'

type PositionCardsProps = {
  sessions: readonly SessionInfo[]
  openPosition?: string
  onOpen: (position: string) => void
}

export function PositionCards({ sessions, openPosition, onOpen }: PositionCardsProps) {
  if (sessions.length === 0) return <p className="text-muted-foreground">No one is running.</p>
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(var(--sidebar),1fr))] gap-3">
      {sessions.map(session => (
        <button
          key={session.position}
          type="button"
          onClick={() => onOpen(session.position)}
          className={cn(
            'flex flex-col gap-1 rounded-lg border border-border bg-background p-4 text-left hover:bg-layer-2',
            session.position === openPosition && 'bg-selection',
          )}
        >
          <span className="flex items-center gap-2 font-medium">
            <span className="dot" style={{ color: ACTIVITY_COLORS[session.activity] }} />
            {session.position}
          </span>
          <span className="text-muted-foreground">
            {ACTIVITY_LABELS[session.activity]}
            {session.mission !== null && <span className="font-mono"> · m{session.mission}</span>}
          </span>
          {!session.can_see_state && <span className="text-warning">can't see this session's state</span>}
        </button>
      ))}
    </div>
  )
}
