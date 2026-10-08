// The channels in one place: the one this machine hosts and the ones it
// subscribes to, the teams reachable over them, and the log of everything
// they carried and saw. Each team has its own color throughout, so who's
// who is easy to tell apart.

import { Copy } from 'lucide-react'
import { type ReactNode, useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import type { ChannelLogEntry } from '@/generated/ChannelLogEntry'
import type { ChannelLogKind } from '@/generated/ChannelLogKind'
import type { Channels } from '@/generated/Channels'
import { clockTime } from '@/lib/format'
import { askFor } from '@/lib/legion'
import { partiesIn, partyColors, type PartyColors } from '@/lib/party-colors'
import { cn } from '@/lib/utils'

// The newest entries read at once; older ones stay in legion2d's log file.
const LOG_LIMIT = 500

const KIND_LABELS: Record<ChannelLogKind, string> = {
  sent: 'sent',
  delivered: 'delivered',
  passed_on: 'passed on',
  undelivered: "didn't get through",
  channel_opened: 'channel opened',
  channel_closed: 'channel closed',
  subscriber_joined: 'subscriber joined',
  subscriber_left: 'subscriber left',
  subscription_up: 'subscription up',
  subscription_down: 'subscription down',
  team_appeared: 'team appeared',
  team_gone: 'team gone',
}

const KIND_GROUPS = {
  all: [],
  messages: ['sent', 'delivered', 'passed_on'],
  problems: ['undelivered', 'subscription_down'],
  connections: ['channel_opened', 'channel_closed', 'subscriber_joined', 'subscriber_left', 'subscription_up', 'subscription_down'],
  teams: ['team_appeared', 'team_gone'],
} as const satisfies Record<string, readonly ChannelLogKind[]>

type KindGroup = keyof typeof KIND_GROUPS
const isKindGroup = (value: string): value is KindGroup => value in KIND_GROUPS

const ALL_TEAMS = 'all'

function MachineTag({ machine }: { machine: string | null }) {
  return machine === null ? null : <span className="whitespace-nowrap">{machine}</span>
}

// A team's name in its own color, its key on hover.
function TeamTag({ teamKey, name, colors }: { teamKey: string; name: string; colors: PartyColors }) {
  return (
    <span className="inline-flex items-center gap-2 whitespace-nowrap" style={{ color: colors(teamKey) }} title={teamKey}>
      <span className="dot" />
      {name}
    </span>
  )
}

function StatusDot({ isUp }: { isUp: boolean }) {
  return <span className="dot" style={{ color: isUp ? 'var(--success)' : 'var(--danger)' }} />
}

function ChannelSummary({ channels }: { channels: Channels }) {
  const { hosted, subscriptions } = channels
  if (hosted === null && subscriptions.length === 0) {
    return <p className="m-0 text-muted-foreground">No channels yet. Open one with `legion2 channel open`, or subscribe to another Legion's with `legion2 channel subscribe`.</p>
  }
  return (
    <div className="flex flex-col gap-3">
      {hosted !== null && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3">
            <StatusDot isUp={hosted.problem === null} />
            <span>Hosting a channel on port {hosted.port}</span>
            <span className="font-mono text-label text-muted-foreground">key {hosted.key}</span>
            <Button variant="ghost" size="icon-xs" aria-label="Copy the key" title="Copy the key" onClick={() => void navigator.clipboard.writeText(hosted.key)}>
              <Copy className="size-4" />
            </Button>
          </div>
          {hosted.problem !== null && <p className="m-0 text-danger">{hosted.problem}</p>}
          <div className="flex flex-wrap gap-4 pl-5">
            {hosted.subscribers.length === 0 ? (
              <span className="text-muted-foreground">No one subscribed yet.</span>
            ) : (
              hosted.subscribers.map(end => (
                <span key={end.address} title={end.address}>
                  <MachineTag machine={end.machine} />
                </span>
              ))
            )}
          </div>
        </div>
      )}
      {subscriptions.map(subscription => (
        <div key={subscription.address} className="flex items-center gap-3" title={subscription.problem ?? undefined}>
          <StatusDot isUp={subscription.is_up} />
          <span>Subscribed to</span>
          {subscription.host_machine === null ? <span>{subscription.address}</span> : <MachineTag machine={subscription.host_machine} />}
          <span className="font-mono text-label text-muted-foreground">{subscription.address}</span>
          {!subscription.is_up && <span className="text-label text-danger">{subscription.problem ?? 'connecting'}</span>}
        </div>
      ))}
    </div>
  )
}

// A table with its headings; its rows are the children.
function Table({ headings, children }: { headings: readonly string[]; children: ReactNode }) {
  return (
    <table>
      <thead>
        <tr>
          {headings.map(heading => (
            <th key={heading}>{heading}</th>
          ))}
        </tr>
      </thead>
      <tbody>{children}</tbody>
    </table>
  )
}

function TeamsTable({ channels, colors }: { channels: Channels; colors: PartyColors }) {
  if (channels.deployments.length === 0) return <p className="m-0 text-muted-foreground">No teams reachable.</p>
  return (
    <Table headings={['Team', 'Machine', 'Folder', 'Pipeline', 'What it does']}>
        {channels.deployments.map(team => (
          <tr key={team.key}>
            <td>
              <TeamTag teamKey={team.key} name={team.name} colors={colors} />
            </td>
            <td>
              <MachineTag machine={team.machine} />
            </td>
            <td>{team.folder}</td>
            <td className="font-mono" title={team.operators.join(' → ')}>
              {team.pipeline}
            </td>
            <td className={team.description === null ? 'text-muted-foreground' : undefined}>{team.description ?? 'No description yet.'}</td>
          </tr>
        ))}
    </Table>
  )
}

type LogTableProps = { entries: readonly ChannelLogEntry[]; colors: PartyColors; teamName: (key: string) => string }

function LogTable({ entries, colors, teamName }: LogTableProps) {
  const [openId, setOpenId] = useState<number>()
  if (entries.length === 0) return <p className="m-0 text-muted-foreground">Nothing yet.</p>
  return (
    <Table headings={['Time', 'What', 'Machine', 'Between', 'Detail']}>
        {entries.map(entry => {
          const isOpen = openId === entry.id
          const teams = [entry.from, entry.to].filter((key): key is string => key !== null && key !== '')
          return (
              <tr key={entry.id}>
                <td className="font-mono text-label whitespace-nowrap text-muted-foreground">{clockTime(entry.at_ms)}</td>
                <td className={entry.kind === 'undelivered' || entry.kind === 'subscription_down' ? 'whitespace-nowrap text-danger' : 'whitespace-nowrap'}>{KIND_LABELS[entry.kind]}</td>
                <td>
                  <MachineTag machine={entry.machine} />
                </td>
                <td className="whitespace-nowrap">
                  {teams.map((key, index) => (
                    <span key={key}>
                      {index > 0 && <span className="text-muted-foreground"> → </span>}
                      <TeamTag teamKey={key} name={teamName(key)} colors={colors} />
                    </span>
                  ))}
                </td>
                <td className="max-w-[var(--measure)]">
                  {/* Click to read it whole; again to fold it. */}
                  <button
                    type="button"
                    className={cn('w-full text-left', isOpen ? 'whitespace-pre-wrap' : 'line-clamp-1')}
                    aria-expanded={isOpen}
                    onClick={() => setOpenId(isOpen ? undefined : entry.id)}
                  >
                    {entry.text}
                  </button>
                </td>
              </tr>
          )
        })}
    </Table>
  )
}

export function ChannelsPage({ channels, changeCount }: { channels?: Channels; changeCount: number }) {
  const [entries, setEntries] = useState<ChannelLogEntry[]>([])
  const [problem, setProblem] = useState<string>()
  const [kindGroup, setKindGroup] = useState<KindGroup>('all')
  const [team, setTeam] = useState(ALL_TEAMS)

  // Read again on every change: a new log entry is one.
  useEffect(() => {
    askFor('channel_log', { type: 'channel_log', limit: LOG_LIMIT })
      .then(reply => {
        setEntries(reply.entries)
        setProblem(undefined)
      })
      .catch((error: unknown) => setProblem(String(error)))
  }, [changeCount])

  if (channels === undefined) return <p className="p-4 text-muted-foreground">This legion2d is older than channels; restart it with the new version.</p>

  const colors = partyColors(partiesIn(channels, entries))
  // Names from the teams reachable now, then from the log for those gone.
  const namesFromLog = new Map(entries.filter(entry => entry.kind === 'team_appeared' && entry.from !== null).map(entry => [entry.from ?? '', entry.text.split(' (')[0] ?? '']))
  const namesNow = new Map(channels.deployments.map(deployment => [deployment.key, deployment.name]))
  const teamName = (key: string) => namesNow.get(key) ?? namesFromLog.get(key) ?? key
  const teamKeys = [...new Set(entries.flatMap(entry => [entry.from, entry.to]).filter((key): key is string => key !== null && key !== ''))]
  const kinds = new Set<ChannelLogKind>(KIND_GROUPS[kindGroup])
  const shown = entries
    .filter(entry => kinds.size === 0 || kinds.has(entry.kind))
    .filter(entry => team === ALL_TEAMS || entry.from === team || entry.to === team)
    .reverse()

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-auto p-4">
      <section className="flex flex-col gap-2">
        <span className="label">This machine</span>
        <ChannelSummary channels={channels} />
      </section>
      <section className="flex flex-col gap-2">
        <span className="label">Teams on the channels</span>
        <TeamsTable channels={channels} colors={colors} />
      </section>
      <section className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <span className="label">Log</span>
          <ToggleGroup type="single" variant="outline" value={kindGroup} onValueChange={value => isKindGroup(value) && setKindGroup(value)} aria-label="Show">
            <ToggleGroupItem value="all">All</ToggleGroupItem>
            <ToggleGroupItem value="messages">Messages</ToggleGroupItem>
            <ToggleGroupItem value="problems">Problems</ToggleGroupItem>
            <ToggleGroupItem value="connections">Connections</ToggleGroupItem>
            <ToggleGroupItem value="teams">Teams</ToggleGroupItem>
          </ToggleGroup>
          <Select value={team} onValueChange={setTeam}>
            <SelectTrigger size="sm" aria-label="Team">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL_TEAMS}>Every team</SelectItem>
              {teamKeys.map(key => (
                <SelectItem key={key} value={key}>
                  {teamName(key)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        {problem !== undefined && <p className="m-0 text-danger">{problem}</p>}
        <LogTable entries={shown} colors={colors} teamName={teamName} />
      </section>
    </div>
  )
}
