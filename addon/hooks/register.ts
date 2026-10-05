import type { EngineInterface, Register } from 'claude-code'

// Reports this session's state to legion2d, and hands the session the
// messages legion2d holds for it. legion2d starts the session with
// LEGION_ADDON_SOCKET and LEGION_ADDON_SESSION set; without them (a session
// legion2d didn't start) the add-on does nothing.

// Bumped when what the add-on sends changes, so legion2d can tell it's a
// version it knows.
const ADDON_VERSION = '1'
const INBOX_POLL_MS = 1000

type State = 'idle' | 'busy' | 'permission' | 'ended'

const link: { socket?: string; session?: string; lastSent: string } = { lastSent: '' }

async function post($: EngineInterface, body: Record<string, unknown>) {
  if (link.socket === undefined || link.session === undefined) return
  try {
    await $.http.fetch(`http://legion2d/sessions/${link.session}/state`, {
      method: 'POST',
      socketPath: link.socket,
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    })
  } catch {
    // legion2d isn't listening; the next report tries again.
  }
}

async function report($: EngineInterface, state: State, detail?: string) {
  const key = `${state}|${detail ?? ''}`
  if (key === link.lastSent) return
  link.lastSent = key
  await post($, { state, detail })
}

// prompt.submit waits for the session to be free, so polling while it works
// is fine: a message taken now runs once the current turn ends.
async function takeMessages($: EngineInterface) {
  if (link.socket === undefined || link.session === undefined) return
  try {
    const res = await $.http.fetch(`http://legion2d/sessions/${link.session}/inbox`, { socketPath: link.socket })
    if (!res.ok || res.status === 204) return
    const { messages } = JSON.parse(res.text) as { messages: string[] }
    for (const text of messages) await $.prompt.submit({ text, asUser: true })
  } catch {
    // legion2d isn't listening; try again next tick.
  }
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    link.socket = await $.env.get('LEGION_ADDON_SOCKET')
    link.session = await $.env.get('LEGION_ADDON_SESSION')
    if (link.socket === undefined || link.session === undefined) return next(e)

    const { version } = await $.session.version()
    link.lastSent = 'idle|'
    await post($, { state: 'idle', addon: ADDON_VERSION, claude: version })
    $.clock.every(INBOX_POLL_MS, () => takeMessages($))
    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    await report($, 'busy')
    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    // A subagent's turn ending doesn't free the session.
    if (e.agentId === undefined) await report($, 'idle')
    return next(e)
  })

  // "ask" puts the call to the person: the session waits on a permission
  // question until they answer.
  on('tool.check', async ($, e, next) => {
    const verdict = await next(e)
    if (e.tool_use_id !== undefined && verdict.decision === 'ask') {
      await report($, 'permission', `${e.tool}${verdict.reason ? `: ${verdict.reason}` : ''}`)
    }
    return verdict
  })

  // The permission check runs inside the call, so once the call returns any
  // question it raised has been answered.
  on('tool.call', async ($, e, next) => {
    const result = await next(e)
    if (link.lastSent.startsWith('permission|')) await report($, 'busy')
    return result
  })

  on('session.end', async ($, e, next) => {
    await report($, 'ended', e.reason)
    return next(e)
  })
}
