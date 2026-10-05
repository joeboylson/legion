import { describe, expect, it } from 'vitest'

import type { Entry } from '@/generated/Entry'
import type { Mission } from '@/generated/Mission'
import type { Run } from '@/generated/Run'
import type { SessionInfo } from '@/generated/SessionInfo'

import { needsYouItems, type RunSnapshot } from './needs-you'

const run: Run = { id: 'r1', name: 'feature', folder: '/repo', pipeline: 'feature', started_ms: 0, closed_ms: null }

const session = (position: string, activity: SessionInfo['activity'], detail: string | null = null): SessionInfo => ({
  run: 'r1',
  position,
  mission: 2,
  activity,
  can_see_state: true,
  detail,
})

const entry = (id: number, kind: Entry['kind'], text: string): Entry => ({
  id,
  run: 'r1',
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
  run: 'r1',
  title: `mission ${number}`,
  file: '',
  status,
  holder: null,
})

const snapshot = (overrides: Partial<RunSnapshot>): RunSnapshot => ({
  run,
  sessions: [],
  missions: [],
  openQuestions: [],
  suggestions: [],
  ...overrides,
})

describe('needsYouItems', () => {
  it('is empty when nothing waits', () => {
    expect(needsYouItems([snapshot({ sessions: [session('builder', 'busy')] })], new Set())).toEqual([])
  })

  it('lists permission questions first, then questions, blocked missions, limits and suggestions', () => {
    const items = needsYouItems(
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
    const items = needsYouItems([snapshot({ suggestions: [entry(7, 'suggestion', 'Add tests')] })], new Set(['suggestion:7']))
    expect(items).toEqual([])
  })

  it('leaves out closed runs', () => {
    const closedRun = { ...run, closed_ms: 1 }
    expect(needsYouItems([snapshot({ run: closedRun, sessions: [session('builder', 'permission')] })], new Set())).toEqual([])
  })

  it('says what a permission question is about even without detail', () => {
    const items = needsYouItems([snapshot({ sessions: [session('builder', 'permission')] })], new Set())
    expect(items[0]?.text).toBe('a tool needs permission')
  })
})
