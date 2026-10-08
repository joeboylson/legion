// A whole mission in a dialog, opened from the missions table or the sidebar.

import { useEffect, useState } from 'react'

import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { Mission } from '@/generated/Mission'
import type { Part } from '@/generated/Part'
import { MISSION_STATUS_LABELS } from '@/lib/format'
import { askFor } from '@/lib/legion'

import { Setting, SettingsList } from './SettingsList'

type MissionDialogProps = { mission?: Mission; onClose: () => void }

export function MissionDialog({ mission, onClose }: MissionDialogProps) {
  return (
    <Dialog open={mission !== undefined} onOpenChange={isOpen => !isOpen && onClose()}>
      {mission !== undefined && <MissionContent key={`${mission.deployment}:${mission.number}`} mission={mission} />}
    </Dialog>
  )
}

function MissionContent({ mission }: { mission: Mission }) {
  const [body, setBody] = useState<string>()
  const [parts, setParts] = useState<readonly Part[]>([])

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
