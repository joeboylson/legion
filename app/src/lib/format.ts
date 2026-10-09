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
  halted: 'halted',
  ended: 'ended',
}

// A deployment's pipeline, as its tag reads: pipeline:feature.
// What no pipeline is called where a pipeline's name goes; `hub` is its old name.
export const NO_PIPELINE = 'none'
const OLD_NO_PIPELINE = 'hub'

export const isNoPipeline = (pipeline: string) => pipeline === NO_PIPELINE || pipeline === OLD_NO_PIPELINE

// How a pipeline's name reads: "no pipeline" when there's none.
export const pipelineLabel = (pipeline: string): string => (isNoPipeline(pipeline) ? 'no pipeline' : pipeline)

export const pipelineTag = (pipeline: string): string => (isNoPipeline(pipeline) ? 'no pipeline' : `pipeline:${pipeline}`)

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

// Something is happening: at least one session is at work.
export const isWorking = (sessions: readonly SessionInfo[]): boolean => sessions.some(session => session.activity === 'busy')

// The dot a folder or deployment shows: green while anything in it works.
export const workingDotColor = (isActive: boolean): string => (isActive ? 'var(--success)' : 'var(--fg-muted)')

// The dot's color says whether it needs a look.
export const ACTIVITY_COLORS: Record<Activity, string> = {
  starting: 'var(--fg-muted)',
  idle: 'var(--fg-muted)',
  busy: 'var(--success)',
  permission: 'var(--warning)',
  limited: 'var(--warning)',
  halted: 'var(--warning)',
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
