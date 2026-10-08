// One part of a deployment in its own tab: its operators (with their log),
// questions, decisions, missions, or a part not built yet. A line along the top says which
// deployment it is.

import { EscalationsTable } from '@/components/EscalationsTable'
import { MissionsTab } from '@/components/MissionsTab'
import { NewMissionDialog } from '@/components/NewMissionDialog'
import { OperatorsPane } from '@/components/OperatorsPane'
import type { Mission } from '@/generated/Mission'
import { OPERATOR_KINDS } from '@/lib/blockers'
import type { DeploymentSnapshot, Escalation } from '@/lib/escalations'
import { dayAndTime, pipelineTag } from '@/lib/format'
import type { DeploymentPart } from '@/lib/tabs'

type DeploymentPartViewProps = {
  snapshot: DeploymentSnapshot
  part: DeploymentPart
  // What it has escalated, still waiting.
  escalations: readonly Escalation[]
  changeCount: number
  openPosition?: string
  onOpenPosition: (position: string) => void
  onOpenMission: (mission: Mission) => void
  onOpenEscalation: (escalation: Escalation) => void
}

function PartContent({ snapshot, part, escalations, changeCount, openPosition, onOpenPosition, onOpenMission, onOpenEscalation }: DeploymentPartViewProps) {
  const { deployment } = snapshot
  switch (part) {
    case 'operators':
      return <OperatorsPane snapshot={snapshot} changeCount={changeCount} openPosition={openPosition} onOpen={onOpenPosition} />
    case 'missions':
      return (
        <>
          {deployment.closed_ms === null && (
            <div>
              <NewMissionDialog deploymentId={deployment.id} />
            </div>
          )}
          <MissionsTab missions={snapshot.missions} onOpen={onOpenMission} />
        </>
      )
    case 'questions':
      return (
        <EscalationsTable
          items={escalations.filter(item => item.kind !== 'decision' && !OPERATOR_KINDS.includes(item.kind))}
          showsKind
          emptyText="Nothing waiting on you."
          onOpen={onOpenEscalation}
        />
      )
    case 'decisions':
      return <EscalationsTable items={escalations.filter(item => item.kind === 'decision')} showsKind={false} emptyText="No decisions to look at." onOpen={onOpenEscalation} />
    case 'templates':
      return <p className="text-muted-foreground">Not built yet.</p>
  }
}

export function DeploymentPartView(props: DeploymentPartViewProps) {
  const { deployment } = props.snapshot
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="breadcrumbs flex-none pt-2" title={deployment.folder}>
        <span className="text-foreground">{deployment.name}</span>
        <span className="truncate font-mono text-label">
          {pipelineTag(deployment.pipeline)} · {deployment.id}
          {deployment.closed_ms !== null && ` · closed ${dayAndTime(deployment.closed_ms)}`}
        </span>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4">
        <PartContent {...props} />
      </div>
    </div>
  )
}
