// The Tauri window's way to legion2d: through the app's back end
// (src-tauri/src/main.rs), which holds the connection.

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import type { Command } from '@/generated/Command'
import type { Event } from '@/generated/Event'
import type { Reply } from '@/generated/Reply'

import type { LegionConnection } from './legion-connection'

// Owned by the app's back end.
const EVENT_CHANNEL = 'legion-event'
const CONNECTION_CHANNEL = 'legion-connection'

export const windowConnection: LegionConnection = {
  ask: (command: Command) => invoke<Reply>('legion_request', { command }),
  onEvent: handle => listen<Event>(EVENT_CHANNEL, message => handle(message.payload)),
  onConnectionChange: handle => listen<boolean>(CONNECTION_CHANNEL, message => handle(message.payload)),
}
