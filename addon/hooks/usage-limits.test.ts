import { expect, test } from 'claude-code/testing'

import {
  CARRY_ON_DELAY_AFTER_RESET_MS,
  CARRY_ON_RETRY_WITHOUT_RESET_MS,
  exhaustedWindows,
  limitDetail,
  msUntilCarryOn,
} from './usage-limits'

const NOW_MS = Date.parse('2026-10-05T12:00:00Z')
const fiveHourFull = { kind: 'five_hour', percentUsed: 100, resetsAt: '2026-10-05T14:00:00Z' }
const weekHalf = { kind: 'seven_day', percentUsed: 50, resetsAt: '2026-10-09T00:00:00Z' }

test('windows under the limit are not exhausted', () => {
  expect(exhaustedWindows([weekHalf])).toEqual([])
  expect(msUntilCarryOn([weekHalf], NOW_MS)).toBe(undefined)
})

test('carries on a minute after the reset', () => {
  const twoHoursMs = 2 * 60 * 60_000
  expect(msUntilCarryOn([fiveHourFull, weekHalf], NOW_MS)).toBe(twoHoursMs + CARRY_ON_DELAY_AFTER_RESET_MS)
})

test('waits for the latest of several used-up windows', () => {
  const weekFull = { ...weekHalf, percentUsed: 100 }
  const untilWeekReset = Date.parse(weekFull.resetsAt) - NOW_MS
  expect(msUntilCarryOn([fiveHourFull, weekFull], NOW_MS)).toBe(untilWeekReset + CARRY_ON_DELAY_AFTER_RESET_MS)
})

test('a reset in the past carries on straight after the delay', () => {
  const alreadyReset = { ...fiveHourFull, resetsAt: '2026-10-05T11:00:00Z' }
  expect(msUntilCarryOn([alreadyReset], NOW_MS)).toBe(CARRY_ON_DELAY_AFTER_RESET_MS)
})

test('an unknown reset time retries on a timer', () => {
  const unknownReset = { kind: 'five_hour', percentUsed: 100 }
  expect(msUntilCarryOn([unknownReset], NOW_MS)).toBe(CARRY_ON_RETRY_WITHOUT_RESET_MS)
})

test('the detail names each used-up window', () => {
  expect(limitDetail([fiveHourFull, weekHalf])).toBe('five_hour resets 2026-10-05T14:00:00Z')
})
