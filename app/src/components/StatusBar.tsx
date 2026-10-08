// The bottom bar: whether legion2d is reachable, the channel this machine
// hosts and the ones it subscribes to (each with whether it's up), and how
// many sessions work.

import type { Channels } from '@/generated/Channels'
import type { LegionData } from '@/hooks/useLegion'

const statusColor = (isUp: boolean) => (isUp ? 'var(--success)' : 'var(--danger)')

function ChannelStatus({ channels, onOpen }: { channels: Channels; onOpen: () => void }) {
  const { hosted, subscriptions } = channels
  if (hosted === null && subscriptions.length === 0) return null
  // The whole status opens the Channels tab.
  return (
    <button type="button" className="cursor-pointer hover:text-foreground" onClick={onOpen} title="Open the channels">
      {hosted !== null && (
        <span title={hosted.problem ?? `Key: ${hosted.key}`}>
          <span className="dot" style={{ color: statusColor(hosted.problem === null) }} />
          {hosted.problem === null ? `hosting a channel on :${hosted.port} · ${hosted.subscribers.length} subscribed` : `channel on :${hosted.port} is down`}
        </span>
      )}
      {subscriptions.map(subscription => (
        <span key={subscription.address} title={subscription.problem ?? subscription.address}>
          <span className="dot" style={{ color: statusColor(subscription.is_up) }} />
          subscribed to {subscription.host_machine ?? subscription.address}
          {!subscription.is_up && ' (down)'}
        </span>
      ))}
    </button>
  )
}

export function StatusBar({ legion, onOpenChannels }: { legion: LegionData; onOpenChannels: () => void }) {
  const busyCount = legion.snapshots.flatMap(snapshot => snapshot.sessions).filter(session => session.activity === 'busy').length
  const problem = legion.problem === undefined ? '' : `: ${legion.problem}`
  const connection = legion.isConnected ? 'legion2d connected' : `can't reach legion2d${problem}`
  return (
    // .wb-status places itself in the "status" area.
    <footer className="wb-status">
      <span>
        <span className="dot" style={{ color: statusColor(legion.isConnected) }} />
        {connection}
      </span>
      {legion.channels !== undefined && <ChannelStatus channels={legion.channels} onOpen={onOpenChannels} />}
      <span className="spacer" />
      <span>{busyCount} working</span>
    </footer>
  )
}
