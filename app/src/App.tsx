// One screen for every Legion: each folder's deployments and escalations on
// the left; on the right, a deployment, a folder or a question.

import { useEffect, useState } from 'react'

import { DeploymentView } from '@/components/DeploymentView'
import { FolderPage } from '@/components/FolderPage'
import { QuestionPage } from '@/components/QuestionPage'
import { AppSidebar } from '@/components/AppSidebar'
import { StatusBar } from '@/components/StatusBar'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { useLegion } from '@/hooks/useLegion'
import { useSidebarWidth } from '@/hooks/useSidebarWidth'
import { type Escalation, escalationsIn } from '@/lib/escalations'
import { hashToRoute, routeToHash } from '@/lib/route'
import { type DeploymentTab, mainView, type Selection, selectedRowKey } from '@/lib/selection'
import { readStartingView } from '@/lib/snapshot'

// Where each kind of escalation is dealt with.
const TAB_FOR_ITEM: Record<Escalation['kind'], DeploymentTab> = {
  stuck: 'operators',
  question: 'missions',
  permission: 'operators',
  limit: 'operators',
  blocked: 'missions',
  suggestion: 'log',
}

// A debug starting view that opens the Operators tab without a terminal.
const OPERATORS_TAB_VIEW = 'operators'

const POSITION_ITEM_KINDS: readonly Escalation['kind'][] = ['stuck', 'permission', 'limit']

export function App() {
  const legion = useLegion()
  const sidebar = useSidebarWidth()
  // A reload comes back to the view the address names.
  const [selection, setSelection] = useState<Selection>(() => hashToRoute(window.location.hash).selection)
  const [deploymentTab, setDeploymentTab] = useState<DeploymentTab>(() => hashToRoute(window.location.hash).deploymentTab)

  useEffect(() => {
    window.history.replaceState(null, '', routeToHash({ selection, deploymentTab }) || window.location.pathname)
  }, [selection, deploymentTab])
  const [openPosition, setOpenPosition] = useState<string>()
  const [dismissedKeys, setDismissedKeys] = useState<ReadonlySet<string>>(new Set())

  useEffect(() => {
    void readStartingView().then(view => {
      if (view === undefined) return
      // Debug builds: question:<entry>, folder:<path>, a deployment, its
      // Operators tab (<deployment>/operators), or a position's terminal.
      if (view.deployment.startsWith('question:')) return setSelection({ questionKey: view.deployment })
      if (view.deployment.startsWith('folder:')) return setSelection({ folderPath: view.deployment.slice('folder:'.length) })
      setSelection({ deploymentId: view.deployment })
      if (view.position === null) return
      setDeploymentTab('operators')
      if (view.position !== OPERATORS_TAB_VIEW) setOpenPosition(view.position)
    })
  }, [])

  const escalations = escalationsIn(legion.snapshots, dismissedKeys)
  const view = mainView(selection)
  // A deployment is chosen by its ID; one opened at start may be named instead.
  const shownDeployment = legion.snapshots.find(
    snapshot => view.kind === 'deployment' && (snapshot.deployment.id === view.id || snapshot.deployment.name === view.id),
  )
  const shownQuestion = escalations.find(item => view.kind === 'question' && item.key === view.key)

  const openItem = (item: Escalation) => {
    setSelection(item.kind === 'question' ? { questionKey: item.key } : { deploymentId: item.deploymentId })
    setDeploymentTab(TAB_FOR_ITEM[item.kind])
    setOpenPosition(POSITION_ITEM_KINDS.includes(item.kind) ? item.position : undefined)
  }
  const openOperator = (position?: string) => {
    setDeploymentTab('operators')
    setOpenPosition(position)
  }
  const selectDeployment = (deploymentId: string) => {
    setSelection({ deploymentId })
    setOpenPosition(undefined)
  }
  const closeQuestion = () => setSelection({})

  const emptyMessage = legion.folders.length === 0 ? 'Add a folder to get started.' : 'Choose a deployment, or start one.'
  const main =
    shownQuestion !== undefined ? (
      <QuestionPage key={shownQuestion.key} question={shownQuestion} onAnswered={closeQuestion} onBack={closeQuestion} />
    ) : view.kind === 'folder' ? (
      <FolderPage
        key={view.path}
        folderPath={view.path}
        tab={view.tab}
        onTabChange={tab => setSelection({ folderPath: view.path, folderTab: tab })} changeCount={legion.changeCount} onOpenDeployment={deployment => selectDeployment(deployment.id)} />
    ) : shownDeployment !== undefined ? (
      <DeploymentView
        snapshot={shownDeployment}
        changeCount={legion.changeCount}
        tab={deploymentTab}
        onTabChange={setDeploymentTab}
        openPosition={openPosition}
        onOpenPosition={openOperator}
      />
    ) : (
      <div className="grid flex-1 place-items-center text-muted-foreground">{emptyMessage}</div>
    )

  return (
    <SidebarProvider className="h-full min-h-0" style={{ '--sidebar-width': `${sidebar.widthPx}px` } as React.CSSProperties}>
      <AppSidebar
        folders={legion.folders}
        snapshots={legion.snapshots}
        escalations={escalations}
        selectedKey={selectedRowKey(view)}
        onFolderAdded={legion.reload}
        onOpenFolder={(folder, tab) => setSelection({ folderPath: folder.path, folderTab: tab })}
        onSelectDeployment={deployment => selectDeployment(deployment.id)}
        onOpenOperator={(deployment, position) => {
          setSelection({ deploymentId: deployment.id })
          openOperator(position)
        }}
        onOpenEscalation={openItem}
        onDismissEscalation={item => setDismissedKeys(keys => new Set([...keys, item.key]))}
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
        <main className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">{main}</main>
        <StatusBar legion={legion} />
      </SidebarInset>
    </SidebarProvider>
  )
}
