import { describe, expect, it } from 'vitest'

import type { Entry } from '@/generated/Entry'

import { missionPath } from './mission-path'

const entry = (id: number, from: string, kind: Entry['kind'], text = ''): Entry => ({ id, deployment: 'd', at_ms: id, mission: 7, from, to: null, kind, text, answers: null })

describe('a mission’s way', () => {
  it('reads each status change as a step, and skips the rest', () => {
    const steps = missionPath([entry(1, 'human', 'mission_added'), entry(2, 'poet', 'session_started'), entry(3, 'poet', 'note', 'hm'), entry(4, 'poet', 'handoff', '→ editor: done'), entry(5, 'editor', 'done')])
    expect(steps.map(step => step.text)).toEqual(['added', 'poet started on it', 'poet handed it to editor', 'editor reported it done'])
  })
})
