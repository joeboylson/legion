// When a session that hit its usage limit should be told to carry on.

export type RateLimitWindow = {
  kind: string
  percentUsed: number
  resetsAt?: string
}

const FULL_PERCENT = 100
// A minute past the reset, so the window has surely reopened.
export const CARRY_ON_DELAY_AFTER_RESET_MS = 60_000
// Without a reset time, try again this often.
export const CARRY_ON_RETRY_WITHOUT_RESET_MS = 30 * 60_000

export const CARRY_ON_PROMPT = 'Your usage limit should have reset now. Carry on from where you stopped.'

export const exhaustedWindows = (windows: readonly RateLimitWindow[]): RateLimitWindow[] =>
  windows.filter(window => window.percentUsed >= FULL_PERCENT)

const resetTimeMs = (window: RateLimitWindow): number => (window.resetsAt === undefined ? NaN : Date.parse(window.resetsAt))

// How long to wait before telling the session to carry on, or undefined
// when no window is used up.
export const msUntilCarryOn = (windows: readonly RateLimitWindow[], nowMs: number): number | undefined => {
  const exhausted = exhaustedWindows(windows)
  if (exhausted.length === 0) return undefined
  const resetTimes = exhausted.map(resetTimeMs)
  const hasUnknownReset = resetTimes.some(time => !Number.isFinite(time))
  if (hasUnknownReset) return CARRY_ON_RETRY_WITHOUT_RESET_MS
  const latestReset = Math.max(...resetTimes)
  return Math.max(0, latestReset - nowMs) + CARRY_ON_DELAY_AFTER_RESET_MS
}

// What legion2d is told about the wait.
export const limitDetail = (windows: readonly RateLimitWindow[]): string =>
  exhaustedWindows(windows)
    .map(window => `${window.kind} resets ${window.resetsAt ?? 'at an unknown time'}`)
    .join('; ')
