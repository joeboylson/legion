// How the screens reach legion2d, whichever way the app runs: in its Tauri
// window, or in a browser tab served by legion2d itself.

import type { Command } from '@/generated/Command'
import type { Event } from '@/generated/Event'
import type { Reply } from '@/generated/Reply'

export type StopListening = () => void

export type LegionConnection = {
  ask: (command: Command) => Promise<Reply>
  onEvent: (handle: (event: Event) => void) => Promise<StopListening>
  onConnectionChange: (handle: (isConnected: boolean) => void) => Promise<StopListening>
}

// Tauri puts this on the window; a browser tab doesn't have it.
const TAURI_MARKER = '__TAURI_INTERNALS__'

export const isInAppWindow = (): boolean => typeof window !== 'undefined' && TAURI_MARKER in window
