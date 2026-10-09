import { describe, expect, it } from 'vitest'

import { back, backTo, canGoBack, type Filled, nextStep, stepsShown, withAnswer } from './flow'
import type { Action } from './types'

const action: Action = {
  id: 'mission.add',
  group: 'Mission',
  label: 'Add a mission',
  subjects: ['deployment'],
  steps: [
    { key: 'deployment', kind: 'pick', prompt: 'Which deployment?', choices: () => [], empty: '' },
    { key: 'route', kind: 'pick', prompt: 'How?', choices: () => [], empty: '', when: answers => answers.deployment === 'new' },
    { key: 'title', kind: 'text', prompt: 'Title' },
  ],
  run: () => Promise.resolve(''),
}

describe('an action’s steps', () => {
  it('skips what was given and what doesn’t apply', () => {
    const given: Filled[] = [{ key: 'deployment', value: 'hub-test', how: 'given' }]
    expect(nextStep(action, given)?.key).toBe('title')
    expect(nextStep(action, [])?.key).toBe('deployment')
    expect(nextStep(action, withAnswer([], 'deployment', 'new', 'chosen'))?.key).toBe('route')
    expect(nextStep(action, [...given, { key: 'title', value: 'Haiku', how: 'chosen' }])).toBeUndefined()
  })

  it('goes back to the last choice, skipping what was filled in for you', () => {
    const filled: Filled[] = [
      { key: 'folder', value: '/app', how: 'given' },
      { key: 'deployment', value: 'new', how: 'chosen' },
      { key: 'route', value: 'none', how: 'auto' },
    ]
    expect(back(filled)).toEqual([{ key: 'folder', value: '/app', how: 'given' }])
    expect(canGoBack(back(filled))).toBe(false)
    expect(back([{ key: 'folder', value: '/app', how: 'given' }])).toHaveLength(1)
  })

  it('goes back to any step, keeping what came before it and what was given', () => {
    const filled: Filled[] = [
      { key: 'deployment', value: 'new', how: 'chosen' },
      { key: 'folder', value: '/app', how: 'given' },
      { key: 'route', value: 'none', how: 'chosen' },
      { key: 'title', value: 'Haiku', how: 'chosen' },
    ]
    expect(backTo(filled, 'route').map(entry => entry.key)).toEqual(['deployment', 'folder'])
  })

  it('shows the steps that apply so far', () => {
    expect(stepsShown(action, []).map(step => step.key)).toEqual(['deployment', 'title'])
    expect(stepsShown(action, withAnswer([], 'deployment', 'new', 'chosen')).map(step => step.key)).toEqual(['deployment', 'route', 'title'])
  })
})
