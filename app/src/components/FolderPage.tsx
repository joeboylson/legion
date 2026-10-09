// Everything about one folder, in tabs like a deployment's: its deployments,
// pipelines, operators (click one for its definition) and settings.

import { useEffect, useState } from 'react'

import { ActionButton, ActionContextMenu, ActionMenuButton } from '@/components/actions/ActionMenus'
import { OperatorDialog } from '@/components/OperatorDialog'
import { Setting, SettingsList } from '@/components/SettingsList'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { FolderDetail } from '@/generated/FolderDetail'
import type { PipelineDetail } from '@/generated/PipelineDetail'
import type { Deployment } from '@/generated/Deployment'
import { pipelineLabel } from '@/lib/format'
import { askFor } from '@/lib/legion'
import { byOperatorOrder } from '@/lib/roster'
import { type FolderTab, isFolderTab } from '@/lib/tabs'

type FolderPageProps = {
  folderPath: string
  tab: FolderTab
  onTabChange: (tab: FolderTab) => void
  changeCount: number
  onOpenDeployment: (deployment: Deployment) => void
}

// A pipeline with a file can be edited or removed; no pipeline can't.
function PipelineCard({ folder, pipeline }: { folder: string; pipeline: PipelineDetail }) {
  const hasFile = pipeline.file_text !== null
  const prefill = { folder, pipeline: pipeline.name }
  const card = (
    <article className="flex flex-col gap-3 rounded-lg border border-border p-4">
      <header className="flex items-baseline gap-3">
        <span className="font-medium">{pipelineLabel(pipeline.name)}</span>
        <span className="font-mono text-muted-foreground">{pipeline.first === null ? `${pipeline.operators.join(', ')} through the commander` : pipeline.operators.join(' → ')}</span>
        {hasFile && (
          <span className="ml-auto">
            <ActionMenuButton subject="pipeline" prefill={prefill} label={pipeline.name} />
          </span>
        )}
      </header>
      {pipeline.problem !== null && <p className="text-danger">{pipeline.problem}</p>}
      {pipeline.first !== null && (
        <SettingsList>
          <Setting name="Starts with" value={pipeline.first} />
        </SettingsList>
      )}
      {pipeline.decisions.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>Operator</th>
              <th>When</th>
              <th>Goes to</th>
            </tr>
          </thead>
          <tbody>
            {pipeline.decisions.map(decision => (
              <tr key={`${decision.operator}:${decision.condition}`}>
                <td className="font-mono">{decision.operator}</td>
                <td>{decision.condition}</td>
                <td className="font-mono">{decision.next}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </article>
  )
  if (!hasFile) return card
  return (
    <ActionContextMenu subject="pipeline" prefill={prefill}>
      {card}
    </ActionContextMenu>
  )
}

function DeploymentsList({ deployments, onOpenDeployment }: { deployments: readonly Deployment[]; onOpenDeployment: (deployment: Deployment) => void }) {
  const openDeployments = deployments.filter(deployment => deployment.closed_ms === null)
  if (openDeployments.length === 0) return <p className="text-muted-foreground">No open deployments.</p>
  return (
    <ul className="tree max-w-[var(--measure)]">
      {openDeployments.map(deployment => (
        <li key={deployment.id}>
          <button type="button" onClick={() => onOpenDeployment(deployment)}>
            {deployment.name}
            <span className="muted">
              {pipelineLabel(deployment.pipeline)} · {deployment.id}
            </span>
          </button>
        </li>
      ))}
    </ul>
  )
}

export function FolderPage({ folderPath, tab, onTabChange, changeCount, onOpenDeployment }: FolderPageProps) {
  const [detail, setDetail] = useState<FolderDetail>()
  const [problem, setProblem] = useState<string>()

  useEffect(() => {
    askFor('folder_detail', { type: 'folder_read', folder: folderPath })
      .then(reply => {
        setDetail(reply.detail)
        setProblem(undefined)
      })
      .catch((error: unknown) => setProblem(String(error)))
  }, [folderPath, changeCount])

  if (problem !== undefined) return <p className="p-5 text-danger">{problem}</p>
  if (detail === undefined) return null
  const prefill = { folder: detail.folder.path }

  return (
    <Tabs value={tab} onValueChange={value => isFolderTab(value) && onTabChange(value)} className="flex min-h-0 flex-1 flex-col gap-0">
      <header className="flex min-w-0 items-center gap-3 border-b border-border px-4 py-3">
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="font-medium">{detail.folder.name}</span>
          <span className="truncate font-mono text-label text-muted-foreground">{detail.folder.path}</span>
        </div>
        <ActionButton actionId="deployment.start" prefill={prefill} variant="default" />
        <ActionButton actionId="operator.add" prefill={prefill} />
        <ActionButton actionId="pipeline.add" prefill={prefill} />
        <ActionMenuButton subject="folder" prefill={prefill} label={detail.folder.name} />
      </header>

      <TabsList className="mx-4 mt-4">
        {/* The same order as the folder's sections in the sidebar. */}
        <TabsTrigger value="deployments">Deployments</TabsTrigger>
        <TabsTrigger value="pipelines">Pipelines</TabsTrigger>
        <TabsTrigger value="operators">Operators</TabsTrigger>
        <TabsTrigger value="settings">Settings</TabsTrigger>
      </TabsList>

      <TabsContent value="pipelines" className="min-h-0 overflow-auto p-4">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(var(--measure),1fr))] gap-4">
          {detail.pipelines.map(pipeline => (
            <PipelineCard key={pipeline.name} folder={detail.folder.path} pipeline={pipeline} />
          ))}
        </div>
      </TabsContent>
      <TabsContent value="operators" className="min-h-0 overflow-auto p-4">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(var(--sidebar),1fr))] gap-3">
          {[...detail.operators].sort((first, second) => byOperatorOrder(first.name, second.name)).map(operator => (
            <OperatorDialog key={operator.name} folder={detail.folder.path} operator={operator} />
          ))}
        </div>
      </TabsContent>
      <TabsContent value="deployments" className="min-h-0 overflow-auto p-4">
        <DeploymentsList deployments={detail.deployments} onOpenDeployment={onOpenDeployment} />
      </TabsContent>
      <TabsContent value="settings" className="min-h-0 overflow-auto p-4">
        <div className="mb-4">
          <ActionButton actionId="folder.settings" prefill={prefill} />
        </div>
        <SettingsList>
          <Setting name="Check" value={detail.check} />
          <Setting name="Permissions" value={detail.permission_mode} />
          <Setting name="Data" value={detail.outside_folder} />
        </SettingsList>
      </TabsContent>
    </Tabs>
  )
}
