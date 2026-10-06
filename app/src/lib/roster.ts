// Everyone in a deployment: the sessions running now and each of the
// pipeline's operators with no session, so the whole squad is always shown.

import type { SessionInfo } from '@/generated/SessionInfo'

// A position, and its session if one is running. No session: inactive.
export type RosterEntry = { position: string; session?: SessionInfo }

// Mirrors COMMANDER in legion2-proto.
export const COMMANDER = 'commander'

// Mirrors legion2d's naming.rs: builder-2 is a copy of builder.
const COPY_SUFFIX = /-\d+$/

export const operatorOfPosition = (position: string): string => position.replace(COPY_SUFFIX, '')

// How every list of operators is ordered: the commander first, then A to Z.
export const byOperatorOrder = (first: string, second: string): number => {
  if (first === second) return 0
  if (first === COMMANDER) return -1
  if (second === COMMANDER) return 1
  return first.localeCompare(second)
}

export const rosterOf = (sessions: readonly SessionInfo[], pipelineOperators: readonly string[]): RosterEntry[] => {
  const runningOperators = new Set(sessions.map(session => operatorOfPosition(session.position)))
  // The commander leads every squad, so it's listed even when not running.
  const squad = [COMMANDER, ...pipelineOperators]
  const inactive = squad.filter(operator => !runningOperators.has(operator)).map(operator => ({ position: operator }))
  return [...sessions.map(session => ({ position: session.position, session })), ...inactive].sort((first, second) =>
    byOperatorOrder(first.position, second.position),
  )
}

// The roster with each operator's copies together, in roster order: one
// group per operator, so views can mark the copies as one.
export type RosterGroup = { operator: string; entries: RosterEntry[] }

export const groupedRoster = (roster: readonly RosterEntry[]): RosterGroup[] => {
  // A Map keeps the order each operator first appears in.
  const byOperator = new Map<string, RosterEntry[]>()
  roster.forEach(entry => {
    const operator = operatorOfPosition(entry.position)
    const entries = byOperator.get(operator)
    if (entries === undefined) byOperator.set(operator, [entry])
    else entries.push(entry)
  })
  return [...byOperator].map(([operator, entries]) => ({ operator, entries }))
}
