// The deployment's operators as cards: every running position, then each of
// the pipeline's operators with no session, shown as inactive. An operator's
// copies share a dashed border. Clicking a
// running one opens its live terminal.

import { ActivityDot } from '@/components/ActivityDot'
import { CommanderCrown } from '@/components/CommanderCrown'
import { INACTIVE_LABEL, sessionStatus, workLabel } from '@/lib/format'
import { groupedRoster, type RosterEntry } from '@/lib/roster'
import { cn } from '@/lib/utils'

type PositionCardsProps = {
  roster: readonly RosterEntry[]
  openPosition?: string
  onOpen: (position: string) => void
}

const CARD = 'flex flex-col gap-1 rounded-lg border border-border bg-background p-4 text-left'

function InactiveCard({ position }: { position: string }) {
  return (
    <div className={cn(CARD, 'text-muted-foreground')}>
      <span className="flex items-center gap-2 font-medium">
        <ActivityDot />
        {position}
      </span>
      <span>{INACTIVE_LABEL}</span>
    </div>
  )
}

// The cards' grid, inside a group as well as outside.
const GRID = 'grid grid-cols-[repeat(auto-fill,minmax(var(--sidebar),1fr))] gap-3'

type PositionCardProps = { entry: RosterEntry; isOpen: boolean; onOpen: (position: string) => void }

function PositionCard({ entry: { position, session }, isOpen, onOpen }: PositionCardProps) {
  if (session === undefined) return <InactiveCard position={position} />
  return (
    <button type="button" onClick={() => onOpen(position)} className={cn(CARD, 'hover:bg-layer-2', isOpen && 'bg-selection')}>
      <span className="flex items-center gap-2 font-medium">
        <ActivityDot session={session} />
        {position}
        <CommanderCrown position={position} />
      </span>
      <span className="text-muted-foreground">
        {sessionStatus(session)}
        {workLabel(session) !== undefined && <span className="font-mono"> · {workLabel(session)}</span>}
      </span>
      {!session.can_see_state && <span className="text-warning">can't see this session's state</span>}
    </button>
  )
}

export function PositionCards({ roster, openPosition, onOpen }: PositionCardsProps) {
  if (roster.length === 0) return <p className="text-muted-foreground">No one is running.</p>
  const cardFor = (entry: RosterEntry) => <PositionCard key={entry.position} entry={entry} isOpen={entry.position === openPosition} onOpen={onOpen} />
  return (
    <div className={GRID}>
      {groupedRoster(roster).map(({ operator, entries }) =>
        entries.length === 1 ? (
          entries.map(cardFor)
        ) : (
          // An operator's copies share a border and a row, read as one.
          <section key={operator} className="col-span-full flex flex-col gap-2 rounded-lg border border-dashed border-border p-3">
            <span className="label">{operator}</span>
            <div className={GRID}>{entries.map(cardFor)}</div>
          </section>
        ),
      )}
    </div>
  )
}
