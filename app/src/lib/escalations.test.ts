import { describe, expect, it } from 'vitest'

import type { Entry } from '@/generated/Entry'
import type { Mission } from '@/generated/Mission'
import type { Deployment } from '@/generated/Deployment'
import type { SessionInfo } from '@/generated/SessionInfo'

import { escalationsIn, type DeploymentSnapshot } from './escalations'

const deployment: Deployment = { id: 'r1', name: 'feature', folder: '/repo', pipeline: 'feature', started_ms: 0, closed_ms: null }

const session = (position: string, activity: SessionInfo['activity'], detail: string | null = null): SessionInfo => ({
  deployment: 'r1',
  position,
  mission: 2,
  activity,
  can_see_state: true,
  detail,
  is_stuck_starting: false,
  model: null,
  context_percent: null,
  part: null,
})

const entry = (id: number, kind: Entry['kind'], text: string): Entry => ({
  id,
  deployment: 'r1',
  at_ms: 0,
  mission: null,
  from: 'builder',
  to: 'human',
  kind,
  text,
  answers: null,
})

const mission = (number: number, status: Mission['status']): Mission => ({
  number,
  deployment: 'r1',
  title: `mission ${number}`,
  file: '',
  status,
  holder: null,
})

const snapshot = (overrides: Partial<DeploymentSnapshot>): DeploymentSnapshot => ({
  deployment,
  sessions: [],
  pipelineOperators: [],
  pipelineSteps: [],
  missions: [],
  openQuestions: [],
  suggestions: [],
  ...overrides,
})

describe('escalationsIn', () => {
  it('is empty when nothing waits', () => {
    expect(escalationsIn([snapshot({ sessions: [session('builder', 'busy')] })], new Set())).toEqual([])
  })

  it('lists permission questions first, then questions, blocked missions, limits and suggestions', () => {
    const items = escalationsIn(
      [
        snapshot({
          sessions: [session('reviewer', 'limited'), session('builder', 'permission', 'Bash: rm')],
          openQuestions: [entry(5, 'question', 'Which database?')],
          missions: [mission(3, 'blocked'), mission(4, 'started')],
          suggestions: [entry(7, 'suggestion', 'Add tests')],
        }),
      ],
      new Set(),
    )
    expect(items.map(item => item.kind)).toEqual(['permission', 'question', 'blocked', 'limit', 'suggestion'])
    expect(items[0]?.text).toBe('Bash: rm')
    expect(items[1]?.questionId).toBe(5)
    expect(items[2]?.mission).toBe(3)
  })

  it('leaves out dismissed suggestions', () => {
    const items = escalationsIn([snapshot({ suggestions: [entry(7, 'suggestion', 'Add tests')] })], new Set(['suggestion:7']))
    expect(items).toEqual([])
  })

  it('lists a flagged decision after what holds work up, answerable by its entry', () => {
    const items = escalationsIn(
      [snapshot({ openQuestions: [entry(9, 'decision', 'Used SQLite, not Postgres'), entry(5, 'question', 'Which port?')], suggestions: [entry(7, 'suggestion', 'Add tests')] })],
      new Set(),
    )
    expect(items.map(item => item.kind)).toEqual(['question', 'decision', 'suggestion'])
    expect(items[1]).toMatchObject({ key: 'decision:9', questionId: 9, text: 'Used SQLite, not Postgres' })
  })

  it('leaves out dismissed decisions', () => {
    expect(escalationsIn([snapshot({ openQuestions: [entry(9, 'decision', 'Used SQLite')] })], new Set(['decision:9']))).toEqual([])
  })

  it('leaves out closed deployments', () => {
    const closedDeployment = { ...deployment, closed_ms: 1 }
    expect(escalationsIn([snapshot({ deployment: closedDeployment, sessions: [session('builder', 'permission')] })], new Set())).toEqual([])
  })

  it('puts a session stuck before starting first, with how to unstick it', () => {
    const stuck = { ...session('commander', 'starting'), is_stuck_starting: true }
    const items = escalationsIn([snapshot({ sessions: [session('builder', 'permission'), stuck] })], new Set())
    expect(items.map(item => item.kind)).toEqual(['stuck', 'permission'])
    expect(items[0]?.text).toContain('trust the folder')
  })

  it('says which folder each item is in', () => {
    const items = escalationsIn([snapshot({ sessions: [session('builder', 'permission')] })], new Set())
    expect(items[0]?.folderPath).toBe('/repo')
  })

  it('says what a permission question is about even without detail', () => {
    const items = escalationsIn([snapshot({ sessions: [session('builder', 'permission')] })], new Set())
    expect(items[0]?.text).toBe('a tool needs permission')
  })
})
