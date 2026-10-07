// Escalations: everything sessions have sent up to the human, deployment by
// deployment.

import type { DecisionDetail } from '@/generated/DecisionDetail'
import type { Entry } from '@/generated/Entry'
import type { Mission } from '@/generated/Mission'
import type { Deployment } from '@/generated/Deployment'
import type { SessionInfo } from '@/generated/SessionInfo'

export type EscalationKind = 'stuck' | 'question' | 'permission' | 'limit' | 'blocked' | 'decision' | 'suggestion'

// The kinds you can clear without acting on: nothing waits on them.
export const DISMISSIBLE_KINDS: readonly EscalationKind[] = ['decision', 'suggestion']

// The kinds answered on a page of their own.
export const ANSWERABLE_KINDS: readonly EscalationKind[] = ['question', 'decision']

export type Escalation = {
  key: string
  // The folder whose deployment it's in; the sidebar lists items by folder.
  folderPath: string
  kind: EscalationKind
  deploymentId: string
  deploymentName: string
  position?: string
  mission?: number
  questionId?: number
  text: string
}

// An item before it's given its deployment's folder.
type ItemInDeployment = Omit<Escalation, 'folderPath'>

export type DeploymentSnapshot = {
  deployment: Deployment
  sessions: readonly SessionInfo[]
  // Who its pipeline names, running or not, and who passes work to whom.
  pipelineOperators: readonly string[]
  pipelineSteps: readonly DecisionDetail[]
  missions: readonly Mission[]
  openQuestions: readonly Entry[]
  suggestions: readonly Entry[]
}

// What a session escalation says when the session gave no detail.
const SESSION_ITEM_TEXT: Record<'stuck' | 'permission' | 'limit', string> = {
  stuck: "hasn't started: answer the question on its screen, such as whether to trust the folder",
  permission: 'a tool needs permission',
  limit: 'waiting for its usage limit to reset',
}

const sessionItems = (snapshot: DeploymentSnapshot): ItemInDeployment[] =>
  snapshot.sessions.flatMap(session => {
    const kind: EscalationKind | undefined = session.is_stuck_starting
      ? 'stuck'
      : session.activity === 'permission'
        ? 'permission'
        : session.activity === 'limited'
          ? 'limit'
          : undefined
    if (kind === undefined) return []
    return [
      {
        key: `${kind}:${session.deployment}:${session.position}`,
        kind,
        deploymentId: snapshot.deployment.id,
        deploymentName: snapshot.deployment.name,
        position: session.position,
        mission: session.mission ?? undefined,
        text: session.detail ?? SESSION_ITEM_TEXT[kind],
      },
    ]
  })

// Open questions, and decisions a session made and carried on with: both
// take an answer, but only a question holds anyone up.
const questionItems = (snapshot: DeploymentSnapshot): ItemInDeployment[] =>
  snapshot.openQuestions.map(question => {
    const kind: EscalationKind = question.kind === 'decision' ? 'decision' : 'question'
    return {
      key: `${kind}:${question.id}`,
      kind,
      deploymentId: snapshot.deployment.id,
      deploymentName: snapshot.deployment.name,
      position: question.from,
      mission: question.mission ?? undefined,
      questionId: question.id,
      text: question.text,
    }
  })

const blockedItems = (snapshot: DeploymentSnapshot): ItemInDeployment[] =>
  snapshot.missions
    .filter(mission => mission.status === 'blocked')
    .map(mission => ({
      key: `blocked:${snapshot.deployment.id}:${mission.number}`,
      kind: 'blocked',
      deploymentId: snapshot.deployment.id,
      deploymentName: snapshot.deployment.name,
      mission: mission.number,
      text: mission.title,
    }))

const suggestionItems = (snapshot: DeploymentSnapshot): ItemInDeployment[] =>
  snapshot.suggestions.map(suggestion => ({
    key: `suggestion:${suggestion.id}`,
    kind: 'suggestion',
    deploymentId: snapshot.deployment.id,
    deploymentName: snapshot.deployment.name,
    position: suggestion.from,
    text: suggestion.text,
  }))

// What's most urgent first: a waiting session blocks work right now.
const KIND_ORDER: readonly EscalationKind[] = ['stuck', 'permission', 'question', 'blocked', 'limit', 'decision', 'suggestion']

export const escalationsIn = (snapshots: readonly DeploymentSnapshot[], dismissedKeys: ReadonlySet<string>): Escalation[] =>
  snapshots
    .filter(snapshot => snapshot.deployment.closed_ms === null)
    .flatMap(snapshot =>
      [...sessionItems(snapshot), ...questionItems(snapshot), ...blockedItems(snapshot), ...suggestionItems(snapshot)].map(
        (item): Escalation => ({ ...item, folderPath: snapshot.deployment.folder }),
      ),
    )
    .filter(item => !dismissedKeys.has(item.key))
    .sort((first, second) => KIND_ORDER.indexOf(first.kind) - KIND_ORDER.indexOf(second.kind))
