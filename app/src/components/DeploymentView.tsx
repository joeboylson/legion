// One deployment, with three tabs: its missions, its log, and its operators
// (as a graph, list or grid; click one for its live terminal, in a dialog). Stop and close are in its sidebar menu.

import { LogTab } from '@/components/LogTab'
import { MissionsTab } from '@/components/MissionsTab'
import { NewMissionDialog } from '@/components/NewMissionDialog'
import { OperatorsPane } from '@/components/OperatorsPane'
import { TerminalDialog } from '@/components/TerminalDialog'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { DeploymentSnapshot } from '@/lib/escalations'
import { dayAndTime, pipelineTag } from '@/lib/format'
import { DEPLOYMENT_TABS, type DeploymentTab, isDeploymentTab } from '@/lib/selection'

const TAB_LABELS: Record<DeploymentTab, string> = { missions: 'Missions', log: 'Log', operators: 'Operators' }

type DeploymentViewProps = {
  snapshot: DeploymentSnapshot
  changeCount: number
  tab: DeploymentTab
  onTabChange: (tab: DeploymentTab) => void
  openPosition?: string
  onOpenPosition: (position?: string) => void
}

export function DeploymentView({ snapshot, changeCount, tab, onTabChange, openPosition, onOpenPosition }: DeploymentViewProps) {
  const { deployment } = snapshot
  const isOpenPositionRunning = snapshot.sessions.some(session => session.position === openPosition)

  return (
    <Tabs value={tab} onValueChange={value => isDeploymentTab(value) && onTabChange(value)} className="flex min-h-0 flex-1 flex-col gap-0">
      <header className="flex items-center gap-4 border-b border-border px-4 py-3">
        <div className="flex min-w-0 flex-col">
          <span className="font-medium">{deployment.name}</span>
          <span className="truncate font-mono text-label text-muted-foreground" title={deployment.folder}>
            {pipelineTag(deployment.pipeline)} · {deployment.id}
            {deployment.closed_ms !== null && ` · closed ${dayAndTime(deployment.closed_ms)}`}
          </span>
        </div>
      </header>

      <TabsList className="mx-4 mt-4">
        {DEPLOYMENT_TABS.map(deploymentTab => (
          <TabsTrigger key={deploymentTab} value={deploymentTab}>
            {TAB_LABELS[deploymentTab]}
          </TabsTrigger>
        ))}
      </TabsList>

      <TabsContent value="missions" className="flex min-h-0 flex-col gap-3 overflow-auto p-4">
        {deployment.closed_ms === null && (
          <div>
            <NewMissionDialog deploymentId={deployment.id} />
          </div>
        )}
        <MissionsTab missions={snapshot.missions} />
      </TabsContent>
      <TabsContent value="log" className="min-h-0 overflow-auto p-4">
        <LogTab deploymentId={deployment.id} changeCount={changeCount} />
      </TabsContent>
      <TabsContent value="operators" className="flex min-h-0 flex-col overflow-auto p-4">
        <OperatorsPane snapshot={snapshot} openPosition={openPosition} onOpen={onOpenPosition} />
      </TabsContent>
      <TerminalDialog deploymentId={deployment.id} position={isOpenPositionRunning ? openPosition : undefined} onClose={() => onOpenPosition(undefined)} />
    </Tabs>
  )
}
