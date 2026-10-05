// One screen for every Legion: what needs you and every folder's runs on
// the left, the chosen run on the right.

import { useEffect, useState } from 'react'

import { AddFolderDialog } from '@/components/AddFolderDialog'
import { FolderTree } from '@/components/FolderTree'
import { NeedsYouList } from '@/components/NeedsYouList'
import { RunView } from '@/components/RunView'
import { StartRunDialog } from '@/components/StartRunDialog'
import { useLegion } from '@/hooks/useLegion'
import { needsYouItems, type NeedsYouItem } from '@/lib/needs-you'
import { readStartingView } from '@/lib/snapshot'

export function App() {
  const legion = useLegion()
  const [selectedRunId, setSelectedRunId] = useState<string>()
  const [openPosition, setOpenPosition] = useState<string>()
  const [dismissedKeys, setDismissedKeys] = useState<ReadonlySet<string>>(new Set())

  useEffect(() => {
    void readStartingView().then(view => {
      if (view === undefined) return
      setSelectedRunId(view.run)
      setOpenPosition(view.position ?? undefined)
    })
  }, [])

  const waiting = needsYouItems(legion.snapshots, dismissedKeys)
  // A run is chosen by its ID; one opened at start may be named instead.
  const selected = legion.snapshots.find(snapshot => snapshot.run.id === selectedRunId || snapshot.run.name === selectedRunId)
  const busyCount = legion.snapshots.flatMap(snapshot => snapshot.sessions).filter(session => session.activity === 'busy').length

  const selectRun = (runId: string, position?: string) => {
    setSelectedRunId(runId)
    setOpenPosition(position)
  }
  const openItem = (item: NeedsYouItem) => selectRun(item.runId, item.kind === 'permission' || item.kind === 'limit' ? item.position : undefined)
  const dismissItem = (item: NeedsYouItem) => setDismissedKeys(keys => new Set([...keys, item.key]))

  return (
    <div className="grid h-full grid-cols-[var(--sidebar)_minmax(0,1fr)] grid-rows-[var(--bar)_minmax(0,1fr)_var(--row)] gap-0 [grid-template-areas:'title_title'_'sidebar_main'_'status_status']">
      <header className="flex items-center gap-4 border-b border-border bg-layer-1 px-4 [grid-area:title]">
        <span className="label">Legion</span>
      </header>

      <aside className="flex min-h-0 flex-col overflow-auto border-r border-border bg-layer-1 [grid-area:sidebar]">
        <div className="pane-head">
          <span className="label">Needs you</span>
          {waiting.length > 0 && <span className="count">{waiting.length}</span>}
        </div>
        <NeedsYouList items={waiting} onOpen={openItem} onDismiss={dismissItem} />
        <div className="pane-head">
          <span className="label">Folders</span>
          <span className="flex gap-1">
            <AddFolderDialog onAdded={legion.reload} />
            <StartRunDialog folders={legion.folders} onStarted={run => selectRun(run.id)} />
          </span>
        </div>
        <FolderTree folders={legion.folders} snapshots={legion.snapshots} selectedRunId={selectedRunId} onSelectRun={run => selectRun(run.id)} />
      </aside>

      <main className="flex min-h-0 min-w-0 flex-col bg-background [grid-area:main]">
        {selected === undefined ? (
          <div className="grid flex-1 place-items-center text-muted-foreground">
            {legion.folders.length === 0 ? 'Add a folder to get started.' : 'Choose a run, or start one.'}
          </div>
        ) : (
          <RunView snapshot={selected} changeCount={legion.changeCount} openPosition={openPosition} onOpenPosition={setOpenPosition} />
        )}
      </main>

      {/* .wb-status places itself in the "status" area. */}
      <footer className="wb-status">
        <span>
          <span className="dot" style={{ color: legion.isConnected ? 'var(--success)' : 'var(--danger)' }} />
          {legion.isConnected ? 'legion2d connected' : `can't reach legion2d${legion.problem === undefined ? '' : `: ${legion.problem}`}`}
        </span>
        <span className="spacer" />
        <span>{busyCount} working</span>
      </footer>
    </div>
  )
}
