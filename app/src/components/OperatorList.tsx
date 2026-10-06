// The deployment's operators as a table: one row per position, in the same
// order as the sidebar. Click a running one for its terminal.

import { ActivityDot } from '@/components/ActivityDot'
import { CommanderCrown } from '@/components/CommanderCrown'
import { INACTIVE_LABEL, sessionStatus } from '@/lib/format'
import { groupedRoster, type RosterEntry } from '@/lib/roster'
import { cn } from '@/lib/utils'

type OperatorListProps = { roster: readonly RosterEntry[]; onOpen: (position: string) => void }

export function OperatorList({ roster, onOpen }: OperatorListProps) {
  return (
    <table>
      <thead>
        <tr>
          <th>Operator</th>
          <th>Doing</th>
          <th>Mission</th>
        </tr>
      </thead>
      {/* One body per operator: its copies share a dashed border. */}
      {groupedRoster(roster).map(({ operator, entries }) => (
        <tbody key={operator} className={cn(entries.length > 1 && 'border border-dashed border-border')}>
          {entries.map(({ position, session }) => (
            <tr key={position} className={cn(session === undefined && 'text-muted-foreground')}>
              <td>
                <button
                  type="button"
                  className="flex items-center gap-2 font-mono disabled:cursor-default"
                  disabled={session === undefined}
                  onClick={() => onOpen(position)}
                >
                  <ActivityDot session={session} />
                  {position}
                  <CommanderCrown position={position} />
                </button>
              </td>
              <td>{session === undefined ? INACTIVE_LABEL : sessionStatus(session)}</td>
              <td className="font-mono">{session?.mission ?? ''}</td>
            </tr>
          ))}
        </tbody>
      ))}
    </table>
  )
}
