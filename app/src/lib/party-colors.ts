// A color for each team on the channels, so who's who is easy to see at a
// glance. Teams are colored in key order, so each gets its own color (up to
// the palette's size) and keeps it wherever it shows.

import type { ChannelLogEntry } from '@/generated/ChannelLogEntry'
import type { Channels } from '@/generated/Channels'

// Clearly different hues that read on light and dark alike. No red or
// orange: those mean trouble here.
export const PARTY_PALETTE = ['#3b82f6', '#a855f7', '#14b8a6', '#eab308', '#ec4899', '#84cc16', '#06b6d4', '#8b5cf6', '#f59e0b', '#10b981'] as const

export const UNKNOWN_PARTY_COLOR = 'var(--fg-muted)'

export type PartyColors = (party: string | null | undefined) => string

// Every team, by key, the channels and the log mention.
export const partiesIn = (channels: Channels | undefined, entries: readonly ChannelLogEntry[]): string[] => [
  ...(channels?.deployments.map(deployment => deployment.key) ?? []),
  ...entries.flatMap(entry => [entry.from, entry.to].filter((party): party is string => party !== null && party !== '')),
]

export const partyColors = (parties: readonly string[]): PartyColors => {
  const ordered = [...new Set(parties)].sort((first, second) => first.localeCompare(second))
  return party => {
    const index = party === null || party === undefined ? -1 : ordered.indexOf(party)
    return index === -1 ? UNKNOWN_PARTY_COLOR : (PARTY_PALETTE[index % PARTY_PALETTE.length] ?? UNKNOWN_PARTY_COLOR)
  }
}
