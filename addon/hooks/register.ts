import type { EngineInterface, Register } from 'claude-code'

import { CARRY_ON_PROMPT, limitDetail, msUntilCarryOn, type RateLimitWindow } from './usage-limits'

// Reports this session's state to legion2d, hands the session the messages
// legion2d holds for it, and tells it to carry on once a usage limit resets.
// legion2d starts the session with LEGION_ADDON_SOCKET and
// LEGION_ADDON_SESSION set (names owned by legion2d's constants.rs);
// without them, in a session legion2d didn't start, the add-on does nothing.

// legion2d checks this against the versions it knows (KNOWN_ADDON_VERSIONS).
const ADDON_VERSION = '1'
const INBOX_POLL_MS = 1000
const LEGION_HOST = 'http://legion2d'

type Activity = 'idle' | 'busy' | 'permission' | 'limited' | 'ended'

type Link = {
  socketPath?: string
  sessionId?: string
  lastReported: string
  rateLimits: readonly RateLimitWindow[]
  carryOnTimer?: { cancel: () => void }
}

// Module state: the add-on's one link to legion2d, rebuilt on each load.
const link: Link = { lastReported: '', rateLimits: [] }

const isLinked = (): boolean => link.socketPath !== undefined && link.sessionId !== undefined

const sessionUrl = (route: string): string => `${LEGION_HOST}/sessions/${link.sessionId}/${route}`

async function postToLegion($: EngineInterface, body: Record<string, unknown>) {
  if (!isLinked()) return
  try {
    await $.http.fetch(sessionUrl('state'), {
      method: 'POST',
      socketPath: link.socketPath,
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    })
  } catch {
    // legion2d isn't listening (restarting, say); the next report catches it up.
  }
}

async function reportActivity($: EngineInterface, activity: Activity, detail?: string) {
  const report = `${activity}|${detail ?? ''}`
  if (report === link.lastReported) return
  link.lastReported = report
  await postToLegion($, { state: activity, detail })
}

// prompt.submit waits for the session to be free, so polling while it works
// is fine: a message taken now runs once the current turn ends.
async function deliverMessages($: EngineInterface) {
  if (!isLinked()) return
  try {
    const response = await $.http.fetch(sessionUrl('inbox'), { socketPath: link.socketPath })
    const hasMessages = response.ok && response.status !== 204
    if (!hasMessages) return
    const { messages } = JSON.parse(response.text) as { messages: string[] }
    for (const text of messages) await $.prompt.submit({ text, asUser: true })
  } catch {
    // legion2d isn't listening; try again next tick.
  }
}

async function carryOn($: EngineInterface) {
  link.carryOnTimer = undefined
  await $.prompt.submit({ text: CARRY_ON_PROMPT, asUser: true })
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    link.socketPath = await $.env.get('LEGION_ADDON_SOCKET')
    link.sessionId = await $.env.get('LEGION_ADDON_SESSION')
    if (!isLinked()) return next(e)

    const { version } = await $.session.version()
    link.lastReported = 'idle|'
    await postToLegion($, { state: 'idle', addon: ADDON_VERSION, claude: version })
    $.clock.every(INBOX_POLL_MS, () => deliverMessages($))
    return next(e)
  })

  on('session.measure', async ($, e, next) => {
    link.rateLimits = e.rateLimits
    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    // Carrying on by itself means no nudge is needed.
    link.carryOnTimer?.cancel()
    link.carryOnTimer = undefined
    await reportActivity($, 'busy')
    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    const isMainTurn = e.agentId === undefined
    if (!isMainTurn) return next(e)
    const waitMs = e.reason === 'error' ? msUntilCarryOn(link.rateLimits, Date.now()) : undefined
    if (waitMs === undefined) {
      await reportActivity($, 'idle')
      return next(e)
    }
    link.carryOnTimer?.cancel()
    link.carryOnTimer = $.clock.after(waitMs, () => carryOn($))
    await reportActivity($, 'limited', limitDetail(link.rateLimits))
    return next(e)
  })

  // "ask" puts the call to the person: the session waits on a permission
  // question until they answer.
  on('tool.check', async ($, e, next) => {
    const verdict = await next(e)
    const isRealCall = e.tool_use_id !== undefined
    if (isRealCall && verdict.decision === 'ask') {
      const reason = verdict.reason ? `: ${verdict.reason}` : ''
      await reportActivity($, 'permission', `${e.tool}${reason}`)
    }
    return verdict
  })

  // The permission check runs inside the call, so once the call returns any
  // question it raised has been answered.
  on('tool.call', async ($, e, next) => {
    const result = await next(e)
    if (link.lastReported.startsWith('permission|')) await reportActivity($, 'busy')
    return result
  })

  on('session.end', async ($, e, next) => {
    await reportActivity($, 'ended', e.reason)
    return next(e)
  })
}
