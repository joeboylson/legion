// One screen for every Legion: each folder's deployments and escalations on
// the left; on the right, the pages opened from it, each in a tab.

import { useEffect, useState } from 'react'

import { AppSidebar } from '@/components/AppSidebar'
import { DeploymentPartView } from '@/components/DeploymentPartView'
import { type EditorTabItem, EditorTabs } from '@/components/EditorTabs'
import { FolderPage } from '@/components/FolderPage'
import { BlockerActions } from '@/components/BlockerActions'
import { ChannelsPage } from '@/components/ChannelsPage'
import { EscalationsTable } from '@/components/EscalationsTable'
import { MissionDialog } from '@/components/MissionDialog'
import { QuestionDialog } from '@/components/QuestionDialog'
import { StatusBar } from '@/components/StatusBar'
import { TerminalDialog } from '@/components/TerminalDialog'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { useLegion } from '@/hooks/useLegion'
import { useOpenTabs } from '@/hooks/useOpenTabs'
import { useSidebarWidth } from '@/hooks/useSidebarWidth'
import type { Deployment } from '@/generated/Deployment'
import { type DeploymentSnapshot, type Escalation, escalationsIn } from '@/lib/escalations'
import { blockersIn, OPERATOR_KINDS } from '@/lib/blockers'
import { readStartingView } from '@/lib/snapshot'
import { activeTab, type DeploymentPart, FOLDER_TABS, PART_LABELS, selectedRowKey, type Tab, tabKey } from '@/lib/tabs'

// A debug starting view that opens the Operators tab without a terminal.
const OPERATORS_TAB_VIEW = 'operators'

const POSITION_ITEM_KINDS: readonly Escalation['kind'][] = ['stuck', 'permission', 'halted', 'limit']

type OpenPosition = { deploymentId: string; position: string }
type OpenMission = { deploymentId: string; number: number }

// A deployment is opened by its ID; one opened at start may be named instead.
const findSnapshot = (snapshots: readonly DeploymentSnapshot[], idOrName: string) =>
  snapshots.find(snapshot => snapshot.deployment.id === idOrName || snapshot.deployment.name === idOrName)

const lastPathPart = (path: string) => path.split('/').filter(Boolean).at(-1) ?? path

export function App() {
  const legion = useLegion()
  const sidebar = useSidebarWidth()
  const openTabs = useOpenTabs()
  const [openPosition, setOpenPosition] = useState<OpenPosition>()
  const [openMission, setOpenMission] = useState<OpenMission>()
  const [openQuestionKey, setOpenQuestionKey] = useState<string>()
  const [dismissedKeys, setDismissedKeys] = useState<ReadonlySet<string>>(new Set())
  const { show, close } = openTabs

  const openPart = (deploymentId: string, part: DeploymentPart) => show({ kind: 'deployment', deploymentId, part })
  const openOperator = (deploymentId: string, position: string) => {
    openPart(deploymentId, 'operators')
    setOpenPosition({ deploymentId, position })
  }
  const showMission = (deploymentId: string, number: number) => {
    openPart(deploymentId, 'missions')
    setOpenMission({ deploymentId, number })
  }

  useEffect(() => {
    void readStartingView().then(view => {
      if (view === undefined) return
      // Debug builds: question:<entry>, folder:<path>, a deployment, its
      // Operators tab (<deployment>/operators), or a position's terminal.
      if (view.deployment.startsWith('question:')) return setOpenQuestionKey(view.deployment)
      if (view.deployment.startsWith('folder:')) return show({ kind: 'folder', path: view.deployment.slice('folder:'.length), folderTab: FOLDER_TABS[0] })
      show({ kind: 'deployment', deploymentId: view.deployment, part: 'operators' })
      if (view.position !== null && view.position !== OPERATORS_TAB_VIEW) setOpenPosition({ deploymentId: view.deployment, position: view.position })
    })
  }, [show])

  const escalations = escalationsIn(legion.snapshots, dismissedKeys)
  const shownTab = activeTab(openTabs.open)

  // Over its table: a waiting session's terminal, a blocked mission, or the
  // item itself, to answer or dismiss.
  const openItemOverTable = (item: Escalation) => {
    if (POSITION_ITEM_KINDS.includes(item.kind) && item.position !== undefined) return setOpenPosition({ deploymentId: item.deploymentId, position: item.position })
    if (item.kind === 'blocked' && item.mission !== undefined) return setOpenMission({ deploymentId: item.deploymentId, number: item.mission })
    setOpenQuestionKey(item.key)
  }
  const openItem = (item: Escalation) => {
    openPart(item.deploymentId, item.kind === 'decision' ? 'decisions' : OPERATOR_KINDS.includes(item.kind) ? 'operators' : 'questions')
    openItemOverTable(item)
  }
  const dismiss = (item: Escalation) => setDismissedKeys(keys => new Set([...keys, item.key]))

  const tabLabel = (tab: Tab): string => {
    switch (tab.kind) {
      case 'deployment':
        return `${PART_LABELS[tab.part]} · ${findSnapshot(legion.snapshots, tab.deploymentId)?.deployment.name ?? tab.deploymentId}`
      case 'folder':
        return legion.folders.find(folder => folder.path === tab.path)?.name ?? lastPathPart(tab.path)
      case 'blockers':
        return 'Blockers'
      case 'channels':
        return 'Channels'
    }
  }
  const tabItems: EditorTabItem[] = openTabs.open.tabs.map(tab => {
    const label = tabLabel(tab)
    return { key: tabKey(tab), label, title: tab.kind === 'folder' ? tab.path : label }
  })

  const renderTab = (tab: Tab) => {
    switch (tab.kind) {
      case 'channels':
        return <ChannelsPage channels={legion.channels} changeCount={legion.changeCount} />
      case 'blockers':
        return (
          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4">
            <EscalationsTable
              items={blockersIn(escalations)}
              showsKind
              showsDeployment
              emptyText="No blockers."
              actions={blocker => <BlockerActions blocker={blocker} />}
              onOpen={openItem}
            />
          </div>
        )
      case 'folder':
        return (
          <FolderPage
            key={tab.path}
            folderPath={tab.path}
            tab={tab.folderTab}
            onTabChange={folderTab => show({ ...tab, folderTab })}
            changeCount={legion.changeCount}
            onOpenDeployment={(deployment: Deployment) => openPart(deployment.id, 'operators')}
          />
        )
      case 'deployment': {
        const snapshot = findSnapshot(legion.snapshots, tab.deploymentId)
        if (snapshot === undefined) return <p className="p-4 text-muted-foreground">This deployment isn&apos;t there any more.</p>
        return (
          <DeploymentPartView
            key={tabKey(tab)}
            snapshot={snapshot}
            part={tab.part}
            escalations={escalations.filter(item => item.deploymentId === snapshot.deployment.id)}
            changeCount={legion.changeCount}
            openPosition={openPosition?.deploymentId === snapshot.deployment.id ? openPosition.position : undefined}
            onOpenPosition={position => setOpenPosition({ deploymentId: snapshot.deployment.id, position })}
            onOpenMission={mission => setOpenMission({ deploymentId: snapshot.deployment.id, number: mission.number })}
            onOpenEscalation={openItemOverTable}
          />
        )
      }
    }
  }

  const emptyMessage = legion.folders.length === 0 ? 'Add a folder to get started.' : 'Open something from the sidebar.'
  const terminalSnapshot = openPosition === undefined ? undefined : findSnapshot(legion.snapshots, openPosition.deploymentId)
  // Looked up each time, so the dialog shows where the mission stands now.
  const shownMission =
    openMission === undefined ? undefined : findSnapshot(legion.snapshots, openMission.deploymentId)?.missions.find(mission => mission.number === openMission.number)
  const isTerminalRunning = terminalSnapshot?.sessions.some(session => session.position === openPosition?.position) ?? false

  return (
    <SidebarProvider className="h-full min-h-0" style={{ '--sidebar-width': `${sidebar.widthPx}px` } as React.CSSProperties}>
      <AppSidebar
        folders={legion.folders}
        snapshots={legion.snapshots}
        escalations={escalations}
        channels={legion.channels}
        selectedKey={selectedRowKey(shownTab)}
        onFolderAdded={legion.reload}
        onOpenBlockers={() => show({ kind: 'blockers' })}
        onOpenChannels={() => show({ kind: 'channels' })}
        onOpenFolder={(folder, folderTab) => show({ kind: 'folder', path: folder.path, folderTab: folderTab ?? FOLDER_TABS[0] })}
        onSelectDeployment={deployment => openPart(deployment.id, 'operators')}
        onOpenOperator={(deployment, position) => openOperator(deployment.id, position)}
        onOpenPart={(deployment, part) => openPart(deployment.id, part)}
        onOpenMission={(deployment, mission) => showMission(deployment.id, mission.number)}
        onOpenEscalation={openItem}
        onDismissEscalation={dismiss}
      />
      {/* Drag to resize the sidebar. */}
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize the sidebar"
        aria-valuenow={sidebar.widthPx}
        className="w-1 flex-none cursor-col-resize bg-border hover:bg-highlight"
        onPointerDown={sidebar.startResize}
      />
      <SidebarInset className="flex min-h-0 min-w-0 flex-col">
        <main className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
          {tabItems.length > 0 && <EditorTabs items={tabItems} activeKey={openTabs.open.activeKey} onActivate={openTabs.activate} onClose={close} />}
          {shownTab === undefined ? <div className="grid flex-1 place-items-center text-muted-foreground">{emptyMessage}</div> : renderTab(shownTab)}
        </main>
        <StatusBar legion={legion} onOpenChannels={() => show({ kind: 'channels' })} />
      </SidebarInset>
      <TerminalDialog
        deploymentId={openPosition?.deploymentId ?? ''}
        position={isTerminalRunning ? openPosition?.position : undefined}
        onClose={() => setOpenPosition(undefined)}
      />
      <MissionDialog mission={shownMission} onClose={() => setOpenMission(undefined)} />
      {/* Gone once it's answered, so the dialog closes itself. */}
      <QuestionDialog
        question={escalations.find(item => item.key === openQuestionKey)}
        onClose={() => setOpenQuestionKey(undefined)}
        onDismiss={item => {
          dismiss(item)
          setOpenQuestionKey(undefined)
        }}
      />
    </SidebarProvider>
  )
}
