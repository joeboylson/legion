import type { EngineInterface, Register } from 'claude-code'

import { CARRY_ON_PROMPT, limitDetail, msUntilCarryOn, type RateLimitWindow } from './usage-limits'

// Reports this session's state to legion2d, hands the session the messages
// legion2d holds for it, and tells it to carry on once a usage limit resets.
// legion2d starts the session with LEGION_ADDON_SOCKET and
// LEGION_ADDON_SESSION set, and LEGION_PERMISSION_MODE when the folder sets
// one (names owned by legion2d's constants.rs);
// without them, in a session legion2d didn't start, the add-on does nothing.

// legion2d checks this against the versions it knows (KNOWN_ADDON_VERSIONS).
const ADDON_VERSION = '1'
const INBOX_POLL_MS = 1000
const LEGION_HOST = 'http://legion2d'
// Modes in which Claude, not a person, settles an ask: auto's checker, or
// a mode that never asks. In any other (manual, the default) an ask is a
// question on screen waiting for a person.
// How long a halted session waits for a person to say what to do next
// before it carries on by itself.
const HALT_GRACE_MS = 30_000
const HALTED_CARRY_ON_PROMPT = 'You were stopped and no one has said otherwise. Carry on from where you stopped.'

const SELF_ANSWERING_MODES: readonly string[] = ['auto', 'dontAsk', 'bypassPermissions']

type Activity = 'idle' | 'busy' | 'permission' | 'limited' | 'halted' | 'ended'

type Link = {
  socketPath?: string
  sessionId?: string
  lastReported: string
  rateLimits: readonly RateLimitWindow[]
  // How full the conversation is, in percent; legion2d hands it over past
  // its clearAt.
  contextPercent?: number
  carryOnTimer?: { cancel: () => void }
  // The last call put to the mode's decider, as "Tool: reason".
  lastAsk?: string
  // Whether an ask waits on a person, from the session's permission mode.
  isAskedOfPerson: boolean
  // The turn's latest call was put to a person and came back an error:
  // refused at the dialog. A turn that ends there was stopped by that deny.
  isLastCallRefused: boolean
}

// Module state: the add-on's one link to legion2d, rebuilt on each load.
const link: Link = { lastReported: '', rateLimits: [], isAskedOfPerson: true, isLastCallRefused: false }

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

async function reportActivity($: EngineInterface, activity: Activity, detail?: string, answer?: string) {
  const report = `${activity}|${detail ?? ''}`
  if (report === link.lastReported) return
  link.lastReported = report
  await postToLegion($, { state: activity, detail, answer, context: link.contextPercent })
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

// Stopped by a person: it waits to be told what to do next. If no one does
// (typed in its terminal or sent by Legion, either of which starts a turn
// and cancels this) it carries on by itself, so a halt doesn't leave a
// mission stuck.
async function halt($: EngineInterface) {
  link.carryOnTimer?.cancel()
  link.carryOnTimer = $.clock.after(HALT_GRACE_MS, () => carryOn($, HALTED_CARRY_ON_PROMPT))
  await reportActivity($, 'halted')
}

async function carryOn($: EngineInterface, text: string) {
  link.carryOnTimer = undefined
  await $.prompt.submit({ text, asUser: true })
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    link.socketPath = await $.env.get('LEGION_ADDON_SOCKET')
    link.sessionId = await $.env.get('LEGION_ADDON_SESSION')
    const permissionMode = await $.env.get('LEGION_PERMISSION_MODE')
    link.isAskedOfPerson = permissionMode === undefined || !SELF_ANSWERING_MODES.includes(permissionMode)
    if (!isLinked()) return next(e)

    const { version } = await $.session.version()
    const model = await $.session.model()
    link.lastReported = 'idle|'
    await postToLegion($, { state: 'idle', addon: ADDON_VERSION, claude: version, model })
    $.clock.every(INBOX_POLL_MS, () => deliverMessages($))
    return next(e)
  })

  on('session.measure', async ($, e, next) => {
    link.rateLimits = e.rateLimits
    link.contextPercent = e.context.percent ?? link.contextPercent
    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    // Carrying on by itself means no nudge is needed.
    link.carryOnTimer?.cancel()
    link.carryOnTimer = undefined
    link.isLastCallRefused = false
    await reportActivity($, 'busy')
    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    const isMainTurn = e.agentId === undefined
    if (!isMainTurn) return next(e)
    // Esc mid-turn ends it "aborted"; a deny at the dialog ends it as an
    // ordinary answer right after the refused call.
    if (e.reason === 'aborted' || (e.reason === 'answer' && link.isLastCallRefused)) {
      await halt($)
      return next(e)
    }
    const waitMs = e.reason === 'error' ? msUntilCarryOn(link.rateLimits, Date.now()) : undefined
    if (waitMs === undefined) {
      // legion2d passes on what an operator wrote if it reported nothing.
      await reportActivity($, 'idle', undefined, e.answer)
      return next(e)
    }
    link.carryOnTimer?.cancel()
    link.carryOnTimer = $.clock.after(waitMs, () => carryOn($, CARRY_ON_PROMPT))
    await reportActivity($, 'limited', limitDetail(link.rateLimits))
    return next(e)
  })

  // "ask" puts the call to the mode's decider. Where that's a person, the
  // question is on screen now: report it. (Claude's PermissionRequest and
  // permission_prompt notification never reach a hooks module, so this is
  // the one sign there is.) Auto mode's checker answers most by itself.
  on('tool.check', async ($, e, next) => {
    const verdict = await next(e)
    if (e.tool_use_id === undefined || verdict.decision !== 'ask') return verdict
    link.lastAsk = `${e.tool}${verdict.reason ? `: ${verdict.reason}` : ''}`
    if (link.isAskedOfPerson) await reportActivity($, 'permission', link.lastAsk)
    return verdict
  })

  // The permission check runs inside the call, so once the call returns any
  // question it raised has been answered.
  on('tool.call', async ($, e, next) => {
    const result = await next(e)
    const wasAsked = link.lastReported.startsWith('permission|')
    if (e.agentId === undefined) link.isLastCallRefused = wasAsked && result.isError === true
    if (wasAsked) await reportActivity($, 'busy')
    return result
  })

  on('session.end', async ($, e, next) => {
    await reportActivity($, 'ended', e.reason)
    return next(e)
  })
}
