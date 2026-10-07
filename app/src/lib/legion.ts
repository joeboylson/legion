// Talking to legion2d, the same way from every screen whether the app runs
// in its own window or in a browser tab.

import type { Command } from '@/generated/Command'
import type { Event } from '@/generated/Event'
import type { Reply } from '@/generated/Reply'

import { browserConnection } from './legion-browser'
import { isInAppWindow, type StopListening } from './legion-connection'
import { windowConnection } from './legion-window'

const connection = isInAppWindow() ? windowConnection : browserConnection

export const askLegion = (command: Command): Promise<Reply> => connection.ask(command)

type ReplyOf<Kind extends Reply['type']> = Extract<Reply, { type: Kind }>

// Asks, and checks the reply is the kind expected.
export const askFor = async <Kind extends Reply['type']>(kind: Kind, command: Command): Promise<ReplyOf<Kind>> => {
  const reply = await askLegion(command)
  if (reply.type !== kind) throw new Error(`legion2d answered ${reply.type}, not ${kind}`)
  return reply as ReplyOf<Kind>
}

export const onLegionEvent = (handle: (event: Event) => void): Promise<StopListening> => connection.onEvent(handle)

export const onConnectionChange = (handle: (isConnected: boolean) => void): Promise<StopListening> => connection.onConnectionChange(handle)
