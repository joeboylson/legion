// The bottom bar: whether legion2d is reachable, and how many sessions work.

import type { LegionData } from '@/hooks/useLegion'

export function StatusBar({ legion }: { legion: LegionData }) {
  const busyCount = legion.snapshots.flatMap(snapshot => snapshot.sessions).filter(session => session.activity === 'busy').length
  const problem = legion.problem === undefined ? '' : `: ${legion.problem}`
  const connection = legion.isConnected ? 'legion2d connected' : `can't reach legion2d${problem}`
  return (
    // .wb-status places itself in the "status" area.
    <footer className="wb-status">
      <span>
        <span className="dot" style={{ color: legion.isConnected ? 'var(--success)' : 'var(--danger)' }} />
        {connection}
      </span>
      <span className="spacer" />
      <span>{busyCount} working</span>
    </footer>
  )
}
