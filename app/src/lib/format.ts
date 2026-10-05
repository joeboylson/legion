// How things read on screen.

import type { Activity } from '@/generated/Activity'
import type { Entry } from '@/generated/Entry'
import type { MissionStatus } from '@/generated/MissionStatus'

// Mirrors Activity::label in legion2-proto, shortened for cards.
export const ACTIVITY_LABELS: Record<Activity, string> = {
  starting: 'starting',
  idle: 'idle',
  busy: 'working',
  permission: 'needs permission',
  limited: 'at usage limit',
  ended: 'ended',
}

// The dot's color says whether it needs a look.
export const ACTIVITY_COLORS: Record<Activity, string> = {
  starting: 'var(--fg-muted)',
  idle: 'var(--fg-muted)',
  busy: 'var(--success)',
  permission: 'var(--warning)',
  limited: 'var(--warning)',
  ended: 'var(--danger)',
}

export const MISSION_STATUS_LABELS: Record<MissionStatus, string> = {
  waiting: 'waiting',
  started: 'in progress',
  handed_off: 'handed off',
  paused: 'paused',
  blocked: 'blocked',
  done: 'done',
}

const timeFormat = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' })

export const clockTime = (atMs: number): string => timeFormat.format(new Date(atMs))

export const entryRecipient = (entry: Entry): string => (entry.to === null ? '' : ` → ${entry.to}`)

export const entryKindLabel = (entry: Entry): string => entry.kind.replaceAll('_', ' ')
