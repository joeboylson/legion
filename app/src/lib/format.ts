// How things read on screen.

import type { Activity } from '@/generated/Activity'
import type { Entry } from '@/generated/Entry'
import type { MissionStatus } from '@/generated/MissionStatus'
import type { SessionInfo } from '@/generated/SessionInfo'

// Mirrors Activity::label in legion2-proto, shortened for cards.
export const ACTIVITY_LABELS: Record<Activity, string> = {
  starting: 'starting',
  idle: 'idle',
  busy: 'working',
  permission: 'needs permission',
  limited: 'at usage limit',
  ended: 'ended',
}

// A deployment's pipeline, as its tag reads: pipeline:feature.
export const pipelineTag = (pipeline: string): string => `pipeline:${pipeline}`

// A model's family, as Claude names it: claude-sonnet-5-5 reads "sonnet".
export const modelFamily = (model: string): string => model.replace(/^claude-/, '').split(/[-[]/)[0] ?? model

// What a running session shows: its model and how full its conversation is,
// once known, then what it's doing.
export const sessionStatus = (session: SessionInfo): string =>
  [
    session.model === null ? undefined : modelFamily(session.model),
    session.context_percent === null ? undefined : `${session.context_percent}%`,
    ACTIVITY_LABELS[session.activity],
  ]
    .filter(Boolean)
    .join(' · ')

// The work a session is on: its mission, and its part if the mission is split.
export const workLabel = (session: SessionInfo): string | undefined => {
  if (session.mission === null) return undefined
  return session.part === null ? `m${session.mission}` : `m${session.mission} · part ${session.part}`
}

// A pipeline operator with no session running.
export const INACTIVE_LABEL = 'inactive'

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

const dayFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })

export const dayAndTime = (atMs: number): string => dayFormat.format(new Date(atMs))

export const entryRecipient = (entry: Entry): string => (entry.to === null ? '' : ` → ${entry.to}`)

// A Markdown file's paragraphs, each on one line: its own line breaks are
// for the editor, not the reader. Headings stay paragraphs of their own.
export const paragraphs = (markdown: string): string[] =>
  markdown
    .split(/\n\s*\n/)
    .map(block => block.split('\n').map(line => line.trim()).join(' ').trim())
    .filter(block => block !== '')

export const entryKindLabel = (entry: Entry): string => entry.kind.replaceAll('_', ' ')
