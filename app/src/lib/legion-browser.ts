// A browser tab's way to legion2d: its WebSocket, on the same local address
// that served the page. One connection carries every request and, once it
// asks to watch, every event. It comes back by itself when legion2d
// restarts, so the tab can stay open.

import type { Event } from '@/generated/Event'
import type { Reply } from '@/generated/Reply'
import type { Request } from '@/generated/Request'
import type { ServerMessage } from '@/generated/ServerMessage'

import type { LegionConnection, StopListening } from './legion-connection'

// The same wait as the window's back end.
const RECONNECT_DELAY_MS = 2000
// Owned by legion2d (WEBSOCKET_ROUTE).
const WEBSOCKET_PATH = '/ws'
const WATCH_REQUEST_ID = 'watch'

type Waiting = { resolve: (reply: Reply) => void; reject: (problem: Error) => void }

// The connection is I/O and its state lives here, shared by every screen.
const waiting = new Map<string, Waiting>()
const eventHandlers = new Set<(event: Event) => void>()
const connectionHandlers = new Set<(isConnected: boolean) => void>()
let opening: Promise<WebSocket> | undefined
let requestCount = 0

export const socketAddress = (pageLocation: Pick<Location, 'host'>): string => `ws://${pageLocation.host}${WEBSOCKET_PATH}`

// Settles the request a reply answers; events go to every listener.
export const handleServerMessage = (message: ServerMessage): void => {
  if (message.type === 'event') return eventHandlers.forEach(handle => handle(message.event))
  const request = waiting.get(message.id)
  if (request === undefined) return
  waiting.delete(message.id)
  if ('ok' in message) return request.resolve(message.ok)
  request.reject(new Error(message.error))
}

const tellConnection = (isConnected: boolean) => connectionHandlers.forEach(handle => handle(isConnected))

const send = (socket: WebSocket, request: Request) => socket.send(JSON.stringify(request))

const connect = (): Promise<WebSocket> => {
  if (opening !== undefined) return opening
  opening = new Promise((resolve, reject) => {
    const socket = new WebSocket(socketAddress(window.location))
    socket.onopen = () => {
      send(socket, { id: WATCH_REQUEST_ID, command: { type: 'watch' } })
      tellConnection(true)
      resolve(socket)
    }
    socket.onmessage = message => handleServerMessage(JSON.parse(String(message.data)) as ServerMessage)
    socket.onclose = () => {
      opening = undefined
      const lost = [...waiting.values()]
      waiting.clear()
      lost.forEach(request => request.reject(new Error('lost the connection to legion2d')))
      tellConnection(false)
      reject(new Error("can't reach legion2d"))
      setTimeout(() => void connect().catch(() => undefined), RECONNECT_DELAY_MS)
    }
  })
  return opening
}

const listenTo = <Handler>(handlers: Set<Handler>, handle: Handler): Promise<StopListening> => {
  handlers.add(handle)
  void connect().catch(() => undefined)
  return Promise.resolve(() => void handlers.delete(handle))
}

export const browserConnection: LegionConnection = {
  ask: async command => {
    const socket = await connect()
    requestCount += 1
    const id = String(requestCount)
    const reply = new Promise<Reply>((resolve, reject) => waiting.set(id, { resolve, reject }))
    send(socket, { id, command })
    return reply
  },
  onEvent: handle => listenTo(eventHandlers, handle),
  onConnectionChange: handle => listenTo(connectionHandlers, handle),
}
