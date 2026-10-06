import { describe, expect, it } from 'vitest'

import type { Entry } from '@/generated/Entry'

import { entryKindLabel, entryRecipient, modelFamily, paragraphs, sessionStatus } from './format'

const entry = (overrides: Partial<Entry>): Entry => ({
  id: 1,
  deployment: 'r',
  at_ms: 0,
  mission: null,
  from: 'builder',
  to: null,
  kind: 'note',
  text: '',
  answers: null,
  ...overrides,
})

describe('sessionStatus', () => {
  it('shows the model, how full the conversation is, then the activity', () => {
    const session = { deployment: 'd1', position: 'builder', mission: 8, activity: 'busy', can_see_state: true, detail: null, is_stuck_starting: false, model: 'claude-sonnet-5-5', context_percent: 55 } as const
    expect(sessionStatus(session)).toBe('sonnet · 55% · working')
    expect(sessionStatus({ ...session, model: null, context_percent: null })).toBe('working')
  })
})

describe('modelFamily', () => {
  it('keeps only the family of a model name', () => {
    expect(modelFamily('claude-sonnet-5-5')).toBe('sonnet')
    expect(modelFamily('claude-opus-5-5[1m]')).toBe('opus')
    expect(modelFamily('haiku')).toBe('haiku')
  })
})

describe('paragraphs', () => {
  it('joins wrapped lines and splits on blank lines', () => {
    expect(paragraphs('# Builder\n\nImplement exactly\nwhat you are\nassigned.\n\nSecond.\n')).toEqual([
      '# Builder',
      'Implement exactly what you are assigned.',
      'Second.',
    ])
  })

  it('has nothing for empty text', () => {
    expect(paragraphs('  \n\n ')).toEqual([])
  })
})

describe('entry formatting', () => {
  it('shows a recipient only when there is one', () => {
    expect(entryRecipient(entry({ to: 'commander' }))).toBe(' → commander')
    expect(entryRecipient(entry({}))).toBe('')
  })

  it('reads kinds as words', () => {
    expect(entryKindLabel(entry({ kind: 'session_started' }))).toBe('session started')
  })
})
