import { describe, expect, it } from 'vitest'

import type { Entry } from '@/generated/Entry'

import { entryKindLabel, entryRecipient } from './format'

const entry = (overrides: Partial<Entry>): Entry => ({
  id: 1,
  run: 'r',
  at_ms: 0,
  mission: null,
  from: 'builder',
  to: null,
  kind: 'note',
  text: '',
  answers: null,
  ...overrides,
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
