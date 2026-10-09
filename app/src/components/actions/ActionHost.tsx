// The command menu (⌘K) and every action form, in one place: buttons,
// context menus and the menu itself all open actions here. With nothing
// typed, the menu lists where to go and what to do; picking an action turns
// it into that action's form.

import { useCallback, useEffect, useMemo, useRef } from 'react'

import { ActionForm } from '@/components/actions/ActionForm'
import { CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from '@/components/ui/command'
import type { FolderDetail } from '@/generated/FolderDetail'
import { type ActionState } from '@/hooks/useActions'
import { ACTIONS } from '@/lib/actions/catalog'
import { givenFrom } from '@/lib/actions/flow'
import type { Action, GoTo, Prefill, Sources } from '@/lib/actions/types'
import { askFor } from '@/lib/legion'
import { PART_LABELS } from '@/lib/tabs'

type Destination = { key: string; label: string; hint: string; go: () => void }

// Everything the menu can open a tab to.
const destinationsOf = (sources: Omit<Sources, 'folderDetail'>, goTo: GoTo): Destination[] => {
  const open = sources.snapshots.filter(snapshot => snapshot.deployment.closed_ms === null)
  return [
    { key: 'blockers', label: 'Blockers', hint: 'everything waiting on you', go: goTo.blockers },
    { key: 'channels', label: 'Channels', hint: 'links to other Legions', go: goTo.channels },
    ...sources.folders.map(folder => ({ key: `folder:${folder.path}`, label: folder.name, hint: 'folder', go: () => goTo.folder(folder.path) })),
    ...open.flatMap(({ deployment }) =>
      (['operators', 'missions', 'questions', 'decisions'] as const).map(part => ({
        key: `part:${deployment.id}:${part}`,
        label: `${deployment.name} · ${PART_LABELS[part]}`,
        hint: 'deployment',
        go: () => goTo.deploymentPart(deployment.id, part),
      })),
    ),
    ...open.flatMap(({ deployment, missions }) =>
      missions.map(mission => ({
        key: `mission:${deployment.id}:${mission.number}`,
        label: `#${mission.number} ${mission.title}`,
        hint: `${deployment.name} · ${mission.status.replace('_', ' ')}`,
        go: () => goTo.mission(deployment.id, mission.number),
      })),
    ),
    ...open.flatMap(({ deployment, sessions }) =>
      sessions.map(session => ({
        key: `session:${deployment.id}:${session.position}`,
        label: `${session.position}’s terminal`,
        hint: `${deployment.name} · ${session.activity}`,
        go: () => goTo.session(deployment.id, session.position),
      })),
    ),
  ]
}

function MenuRoot({ sources, onGo, onAction }: { sources: Omit<Sources, 'folderDetail'>; onGo: (destination: Destination) => void; onAction: (action: Action) => void }) {
  const destinations = destinationsOf(sources, sources.goTo)
  return (
    <>
      <CommandInput placeholder="Go to anything, or do something…" autoFocus />
      <CommandList>
        <CommandEmpty>Nothing matches.</CommandEmpty>
        <CommandGroup heading="Actions">
          {ACTIONS.map(action => (
            <CommandItem key={action.id} value={`${action.group} ${action.label}`} onSelect={() => onAction(action)}>
              <span className="w-20 flex-none font-mono text-label text-muted-foreground">{action.group}</span>
              <span className={action.isDestructive === true ? 'text-danger' : undefined}>{action.label}…</span>
            </CommandItem>
          ))}
        </CommandGroup>
        <CommandGroup heading="Go to">
          {destinations.map(destination => (
            <CommandItem key={destination.key} value={`${destination.label} ${destination.hint}`} onSelect={() => onGo(destination)}>
              <span className="truncate">{destination.label}</span>
              <CommandShortcut>{destination.hint}</CommandShortcut>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </>
  )
}

type ActionDialogProps = {
  state: ActionState
  sources: Omit<Sources, 'folderDetail'>
  // What the open tab is about, filled in for actions started from the menu.
  here: Prefill
}

export function ActionDialog({ state, sources, here }: ActionDialogProps) {
  const { view, message, setMessage, close, start } = state
  const details = useRef(new Map<string, Promise<FolderDetail>>())
  const isOpen = view !== undefined

  // A folder's setup is read once per opening, and fresh the next time.
  useEffect(() => {
    if (!isOpen) details.current = new Map()
  }, [isOpen])

  const fullSources = useMemo<Sources>(
    () => ({
      ...sources,
      folderDetail: path => {
        const known = details.current.get(path)
        if (known !== undefined) return known
        const loading = askFor('folder_detail', { type: 'folder_read', folder: path }).then(reply => reply.detail)
        details.current.set(path, loading)
        return loading
      },
    }),
    [sources],
  )

  const finish = useCallback(
    (done: string) => {
      setMessage(done)
      close()
    },
    [setMessage, close],
  )

  return (
    <>
      <CommandDialog
        open={isOpen}
        onOpenChange={open => !open && close()}
        title="Legion"
        description="Go to anything, or do something"
        className="sm:max-w-[var(--measure)]"
        showCloseButton={false}
      >
        {view?.kind === 'menu' && (
          <MenuRoot
            sources={sources}
            onGo={destination => {
              destination.go()
              close()
            }}
            onAction={action => start(action, givenFrom(here))}
          />
        )}
        {view?.kind === 'action' && <ActionForm key={`${view.action.id}:${view.opening}`} action={view.action} initialFilled={view.given} sources={fullSources} onDone={finish} />}
      </CommandDialog>
      {message !== undefined && (
        <div role="status" className="fixed right-4 bottom-10 z-50 max-w-[var(--measure)] rounded-sm border border-border bg-popover px-4 py-3 text-popover-foreground">
          {message}
        </div>
      )}
    </>
  )
}
