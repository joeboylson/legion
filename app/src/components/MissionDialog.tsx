// A mission's title in the missions table; clicking it opens the whole
// mission in a dialog.

import { useState } from 'react'

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import type { Mission } from '@/generated/Mission'
import { MISSION_STATUS_LABELS } from '@/lib/format'
import { askFor } from '@/lib/legion'

import { Setting, SettingsList } from './SettingsList'

export function MissionDialog({ mission }: { mission: Mission }) {
  const [body, setBody] = useState<string>()

  // Read when opened: the body never changes, so once is enough.
  const readBody = (isOpen: boolean) => {
    if (!isOpen || body !== undefined) return
    askFor('mission', { type: 'mission_read', deployment: mission.deployment, mission: mission.number })
      .then(reply => setBody(reply.body))
      .catch((error: unknown) => setBody(`Couldn't read the mission: ${String(error)}`))
  }

  return (
    <Dialog onOpenChange={readBody}>
      <DialogTrigger asChild>
        <button type="button" className="text-left underline-offset-4 hover:underline">
          {mission.title}
        </button>
      </DialogTrigger>
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
        {/* Kept as written: mission bodies often hold numbered steps. */}
        <p className="m-0 whitespace-pre-wrap">{body ?? 'Reading…'}</p>
      </DialogContent>
    </Dialog>
  )
}
