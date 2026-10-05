// Everything waiting on the human, across every run.

import type { Entry } from '@/generated/Entry'
import type { Mission } from '@/generated/Mission'
import type { Run } from '@/generated/Run'
import type { SessionInfo } from '@/generated/SessionInfo'

export type NeedsYouKind = 'question' | 'permission' | 'limit' | 'blocked' | 'suggestion'

export type NeedsYouItem = {
  key: string
  kind: NeedsYouKind
  runId: string
  runName: string
  position?: string
  mission?: number
  questionId?: number
  text: string
}

export type RunSnapshot = {
  run: Run
  sessions: readonly SessionInfo[]
  missions: readonly Mission[]
  openQuestions: readonly Entry[]
  suggestions: readonly Entry[]
}

const sessionItems = (snapshot: RunSnapshot): NeedsYouItem[] =>
  snapshot.sessions.flatMap(session => {
    const kind: NeedsYouKind | undefined =
      session.activity === 'permission' ? 'permission' : session.activity === 'limited' ? 'limit' : undefined
    if (kind === undefined) return []
    return [
      {
        key: `${kind}:${session.run}:${session.position}`,
        kind,
        runId: snapshot.run.id,
        runName: snapshot.run.name,
        position: session.position,
        mission: session.mission ?? undefined,
        text: session.detail ?? (kind === 'permission' ? 'a tool needs permission' : 'waiting for its usage limit to reset'),
      },
    ]
  })

const questionItems = (snapshot: RunSnapshot): NeedsYouItem[] =>
  snapshot.openQuestions.map(question => ({
    key: `question:${question.id}`,
    kind: 'question',
    runId: snapshot.run.id,
    runName: snapshot.run.name,
    position: question.from,
    mission: question.mission ?? undefined,
    questionId: question.id,
    text: question.text,
  }))

const blockedItems = (snapshot: RunSnapshot): NeedsYouItem[] =>
  snapshot.missions
    .filter(mission => mission.status === 'blocked')
    .map(mission => ({
      key: `blocked:${snapshot.run.id}:${mission.number}`,
      kind: 'blocked',
      runId: snapshot.run.id,
      runName: snapshot.run.name,
      mission: mission.number,
      text: mission.title,
    }))

const suggestionItems = (snapshot: RunSnapshot, dismissedKeys: ReadonlySet<string>): NeedsYouItem[] =>
  snapshot.suggestions
    .map(
      (suggestion): NeedsYouItem => ({
        key: `suggestion:${suggestion.id}`,
        kind: 'suggestion',
        runId: snapshot.run.id,
        runName: snapshot.run.name,
        position: suggestion.from,
        text: suggestion.text,
      }),
    )
    .filter(item => !dismissedKeys.has(item.key))

// What's most urgent first: a waiting session blocks work right now.
const KIND_ORDER: readonly NeedsYouKind[] = ['permission', 'question', 'blocked', 'limit', 'suggestion']

export const needsYouItems = (snapshots: readonly RunSnapshot[], dismissedKeys: ReadonlySet<string>): NeedsYouItem[] =>
  snapshots
    .filter(snapshot => snapshot.run.closed_ms === null)
    .flatMap(snapshot => [
      ...sessionItems(snapshot),
      ...questionItems(snapshot),
      ...blockedItems(snapshot),
      ...suggestionItems(snapshot, dismissedKeys),
    ])
    .sort((first, second) => KIND_ORDER.indexOf(first.kind) - KIND_ORDER.indexOf(second.kind))
