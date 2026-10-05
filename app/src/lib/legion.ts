// Talking to legion2d through the app's back end.

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type { Command } from '@/generated/Command'
import type { Event } from '@/generated/Event'
import type { Reply } from '@/generated/Reply'

// Owned by the app's back end (src-tauri/src/main.rs).
const EVENT_CHANNEL = 'legion-event'
const CONNECTION_CHANNEL = 'legion-connection'

export const askLegion = (command: Command): Promise<Reply> => invoke<Reply>('legion_request', { command })

type ReplyOf<Kind extends Reply['type']> = Extract<Reply, { type: Kind }>

// Asks, and checks the reply is the kind expected.
export const askFor = async <Kind extends Reply['type']>(kind: Kind, command: Command): Promise<ReplyOf<Kind>> => {
  const reply = await askLegion(command)
  if (reply.type !== kind) throw new Error(`legion2d answered ${reply.type}, not ${kind}`)
  return reply as ReplyOf<Kind>
}

export const onLegionEvent = (handle: (event: Event) => void): Promise<UnlistenFn> =>
  listen<Event>(EVENT_CHANNEL, message => handle(message.payload))

export const onConnectionChange = (handle: (isConnected: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>(CONNECTION_CHANNEL, message => handle(message.payload))
