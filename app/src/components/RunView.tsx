// One run: its positions, a live terminal for the open one, and tabs for
// its missions and log.

import { useState } from 'react'

import { LogTab } from '@/components/LogTab'
import { MissionsTab } from '@/components/MissionsTab'
import { NewMissionDialog } from '@/components/NewMissionDialog'
import { PositionCards } from '@/components/PositionCards'
import { TerminalView } from '@/components/TerminalView'
import { Button } from '@/components/ui/button'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { askLegion } from '@/lib/legion'
import type { RunSnapshot } from '@/lib/needs-you'

type RunViewProps = {
  snapshot: RunSnapshot
  changeCount: number
  openPosition?: string
  onOpenPosition: (position?: string) => void
}

export function RunView({ snapshot, changeCount, openPosition, onOpenPosition }: RunViewProps) {
  const [isConfirmingClose, setIsConfirmingClose] = useState(false)
  const { run } = snapshot
  const isOpenPositionRunning = snapshot.sessions.some(session => session.position === openPosition)

  return (
    <div className="flex min-h-0 flex-col">
      <header className="flex items-center gap-4 border-b border-border px-4 py-3">
        <div className="flex flex-col">
          <span className="font-medium">{run.name}</span>
          <span className="font-mono text-label text-muted-foreground">
            {run.pipeline} · {run.folder} · {run.id}
          </span>
        </div>
        <span className="flex-1" />
        <NewMissionDialog runId={run.id} />
        {isConfirmingClose ? (
          <>
            <span className="text-muted-foreground">Every session ends.</span>
            <Button variant="destructive" size="sm" onClick={() => void askLegion({ type: 'run_close', run: run.id })}>
              Close run
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setIsConfirmingClose(false)}>
              Keep it
            </Button>
          </>
        ) : (
          <Button variant="ghost" size="sm" onClick={() => setIsConfirmingClose(true)}>
            Close run…
          </Button>
        )}
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-4">
        <PositionCards sessions={snapshot.sessions} openPosition={openPosition} onOpen={onOpenPosition} />

        {openPosition !== undefined && isOpenPositionRunning && (
          <section className="flex flex-col gap-2">
            <div className="flex items-center gap-3">
              <span className="label">terminal · {openPosition}</span>
              <span className="flex-1" />
              <Button variant="ghost" size="xs" onClick={() => onOpenPosition(undefined)}>
                Close terminal
              </Button>
            </div>
            <div className="w-fit rounded-lg border border-border">
              <TerminalView key={`${run.id}:${openPosition}`} runId={run.id} position={openPosition} />
            </div>
          </section>
        )}

        <Tabs defaultValue="missions">
          <TabsList>
            <TabsTrigger value="missions">Missions</TabsTrigger>
            <TabsTrigger value="log">Log</TabsTrigger>
          </TabsList>
          <TabsContent value="missions">
            <MissionsTab missions={snapshot.missions} />
          </TabsContent>
          <TabsContent value="log">
            <LogTab runId={run.id} changeCount={changeCount} />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  )
}
