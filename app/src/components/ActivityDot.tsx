// A position's activity as a colored dot; hollow when it has no session.

import type { SessionInfo } from '@/generated/SessionInfo'
import { ACTIVITY_COLORS } from '@/lib/format'

export function ActivityDot({ session }: { session?: SessionInfo }) {
  if (session === undefined) return <span className="dot flex-none border border-current bg-transparent text-muted-foreground" />
  return <span className="dot flex-none" style={{ color: ACTIVITY_COLORS[session.activity] }} />
}
