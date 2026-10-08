// The Operators tab: the deployment's operators as a live graph, a list or
// a grid of cards, or the log of what they've said, switched at the top.

import { LayoutGrid, List, Network, ScrollText } from 'lucide-react'
import { useState } from 'react'

import { LogTab } from '@/components/LogTab'
import { OperatorGraph } from '@/components/OperatorGraph'
import { OperatorList } from '@/components/OperatorList'
import { PositionCards } from '@/components/PositionCards'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import type { DeploymentSnapshot } from '@/lib/escalations'
import { isOperatorsView, type OperatorsView, readOperatorsView, saveOperatorsView } from '@/lib/operators-view'
import { rosterOf } from '@/lib/roster'

const VIEW_CHOICES: readonly { view: OperatorsView; label: string; icon: typeof Network }[] = [
  { view: 'graph', label: 'Graph', icon: Network },
  { view: 'list', label: 'List', icon: List },
  { view: 'grid', label: 'Grid', icon: LayoutGrid },
  { view: 'log', label: 'Log', icon: ScrollText },
]

type OperatorsPaneProps = { snapshot: DeploymentSnapshot; changeCount: number; openPosition?: string; onOpen: (position: string) => void }

export function OperatorsPane({ snapshot, changeCount, openPosition, onOpen }: OperatorsPaneProps) {
  const [view, setView] = useState<OperatorsView>(readOperatorsView)
  const roster = rosterOf(snapshot.sessions, snapshot.pipelineOperators)
  const chooseView = (value: string) => {
    if (!isOperatorsView(value)) return
    setView(value)
    saveOperatorsView(value)
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <ToggleGroup type="single" variant="outline" value={view} onValueChange={chooseView} aria-label="Show operators as">
        {VIEW_CHOICES.map(({ view: choice, label, icon: Icon }) => (
          <ToggleGroupItem key={choice} value={choice} aria-label={label}>
            <Icon className="size-4" />
            {label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      {view === 'graph' && <OperatorGraph deploymentId={snapshot.deployment.id} roster={roster} steps={snapshot.pipelineSteps} pipelineOrder={snapshot.pipelineOperators} onOpen={onOpen} />}
      {view === 'list' && <OperatorList roster={roster} onOpen={onOpen} />}
      {view === 'grid' && <PositionCards roster={roster} openPosition={openPosition} onOpen={onOpen} />}
      {view === 'log' && <LogTab deploymentId={snapshot.deployment.id} changeCount={changeCount} />}
    </div>
  )
}
