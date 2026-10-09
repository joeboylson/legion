// A whole mission in a dialog, opened from the missions table or the sidebar,
// with its way through its statuses so far, kept up to date as it moves.

import { useEffect, useState } from 'react'

import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { Mission } from '@/generated/Mission'
import type { Part } from '@/generated/Part'
import { clockTime, MISSION_STATUS_LABELS } from '@/lib/format'
import { askFor } from '@/lib/legion'
import { missionPath, type PathStep } from '@/lib/mission-path'

import { Setting, SettingsList } from './SettingsList'

type MissionDialogProps = { mission?: Mission; changeCount: number; onClose: () => void }

export function MissionDialog({ mission, changeCount, onClose }: MissionDialogProps) {
  return (
    <Dialog open={mission !== undefined} onOpenChange={isOpen => !isOpen && onClose()}>
      {mission !== undefined && <MissionContent key={`${mission.deployment}:${mission.number}`} mission={mission} changeCount={changeCount} />}
    </Dialog>
  )
}

function MissionContent({ mission, changeCount }: { mission: Mission; changeCount: number }) {
  const [body, setBody] = useState<string>()
  const [parts, setParts] = useState<readonly Part[]>([])
  const [path, setPath] = useState<readonly PathStep[]>([])

  // Read again on every change, so its way grows as it moves.
  useEffect(() => {
    const filter = { mission: mission.number, position: null, kinds: null, since_ms: null, open_questions: false }
    askFor('entries', { type: 'log', deployment: mission.deployment, filter })
      .then(reply => setPath(missionPath(reply.entries)))
      .catch(() => setPath([]))
  }, [mission.deployment, mission.number, changeCount])

  // Read each time it opens: its parts merge as the work goes on.
  useEffect(() => {
    askFor('mission', { type: 'mission_read', deployment: mission.deployment, mission: mission.number })
      .then(reply => {
        setBody(reply.body)
        // A legion2d older than parts sends none.
        setParts(reply.parts ?? [])
      })
      .catch((error: unknown) => setBody(`Couldn't read the mission: ${String(error)}`))
  }, [mission.deployment, mission.number])

  return (
    <DialogContent className="max-h-[calc(100vh-var(--space-8))] overflow-auto sm:max-w-[var(--measure)]">
      <DialogHeader>
        <DialogTitle>
          <span className="font-mono text-muted-foreground">#{mission.number}</span> {mission.title}
        </DialogTitle>
      </DialogHeader>
      <SettingsList>
        <Setting name="Where it stands" value={MISSION_STATUS_LABELS[mission.status]} />
        <Setting name="Held by" value={mission.holder} />
        <Setting name="File" value={mission.file} />
      </SettingsList>
      {path.length > 0 && (
        <section className="flex flex-col gap-2">
          <span className="label">Its way so far</span>
          <ol className="m-0 flex list-none flex-col gap-1 p-0">
            {path.map(step => (
              <li key={step.id} className="flex gap-3">
                <span className="font-mono text-label text-muted-foreground">{clockTime(step.atMs)}</span>
                <span>{step.text}</span>
              </li>
            ))}
          </ol>
        </section>
      )}
      {parts.length > 0 && (
        // The commander's split of this mission's current step.
        <section className="flex flex-col gap-2">
          <span className="label">Parts</span>
          <ol className="m-0 flex list-none flex-col gap-1 p-0">
            {parts.map(part => (
              <li key={part.number} className="flex gap-3">
                <span className="font-mono text-muted-foreground">{part.number}</span>
                <span className="flex-1">{part.brief}</span>
                <span className="font-mono text-label text-muted-foreground">{part.is_merged ? 'merged' : 'open'}</span>
              </li>
            ))}
          </ol>
        </section>
      )}
      {/* Kept as written: mission bodies often hold numbered steps. */}
      <p className="m-0 whitespace-pre-wrap">{body ?? 'Reading…'}</p>
    </DialogContent>
  )
}
