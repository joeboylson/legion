// The sidebar, built from shadcn's Sidebar: each local folder, and under it
// its deployments (each with what it has escalated), pipelines and operators.
// Every level folds like a file tree.

import { ChevronRight, Settings, X } from 'lucide-react'
import type { ReactNode } from 'react'

import { ActivityDot } from '@/components/ActivityDot'
import { CommanderCrown } from '@/components/CommanderCrown'
import { DeploymentMenu } from '@/components/DeploymentMenu'
import { NewProjectDialog } from '@/components/NewProjectDialog'
import { StartDeploymentDialog } from '@/components/StartDeploymentDialog'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from '@/components/ui/sidebar'
import type { Deployment } from '@/generated/Deployment'
import type { Folder } from '@/generated/Folder'
import type { DeploymentSnapshot, Escalation, EscalationKind } from '@/lib/escalations'
import { INACTIVE_LABEL, pipelineTag, sessionStatus } from '@/lib/format'
import { byOperatorOrder, type RosterEntry, rosterOf } from '@/lib/roster'
import type { FolderTab } from '@/lib/selection'
import { cn } from '@/lib/utils'

const KIND_LABELS: Record<EscalationKind, string> = {
  stuck: 'start',
  permission: 'permission',
  question: 'question',
  blocked: 'blocked',
  limit: 'usage limit',
  suggestion: 'suggestion',
}

// A little room between items (3px), and every level reaches the same right
// edge so the action column lines up.
const TIGHT_SUBMENU = 'mr-0 translate-x-0 gap-1 pt-1 pb-0 pr-0'

type AppSidebarProps = {
  folders: readonly Folder[]
  snapshots: readonly DeploymentSnapshot[]
  escalations: readonly Escalation[]
  selectedKey?: string
  onFolderAdded: () => void
  onOpenFolder: (folder: Folder, tab?: FolderTab) => void
  onSelectDeployment: (deployment: Deployment) => void
  onOpenOperator: (deployment: Deployment, position: string) => void
  onOpenEscalation: (escalation: Escalation) => void
  onDismissEscalation: (escalation: Escalation) => void
}

// The chevron turns as its Collapsible (named by `group`) opens.
// Spelled out: Tailwind only builds classes it can find written whole.
const TURNS_WHEN_OPEN = {
  folder: 'group-data-[state=open]/folder:rotate-90',
  section: 'group-data-[state=open]/section:rotate-90',
  deployment: 'group-data-[state=open]/deployment:rotate-90',
} as const

function Chevron({ group }: { group: keyof typeof TURNS_WHEN_OPEN }) {
  return <ChevronRight className={`size-4 flex-none transition-transform ${TURNS_WHEN_OPEN[group]}`} />
}

// Every row: its button, then one fixed-width slot for an action (or
// nothing), so actions and counts line up down the whole tree.
function Row({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex items-center gap-1">
      <div className="min-w-0 flex-1">{children}</div>
      <span className="flex size-5 flex-none items-center justify-center">{action}</span>
    </div>
  )
}

function Count({ value }: { value: number }) {
  return <span className="ml-auto flex-none font-mono text-label text-muted-foreground">{value}</span>
}

type SectionProps = { label: string; count: number; action?: ReactNode; children: ReactNode }

// A group under a folder (deployments, pipelines …): a row that folds, and its items.
function Section({ label, count, action, children }: SectionProps) {
  return (
    <Collapsible defaultOpen asChild className="group/section">
      <SidebarMenuSubItem>
        <Row action={action}>
          <CollapsibleTrigger asChild>
            <SidebarMenuSubButton asChild>
              <button type="button" className="w-full">
                <Chevron group="section" />
                <span className="label">{label}</span>
                <Count value={count} />
              </button>
            </SidebarMenuSubButton>
          </CollapsibleTrigger>
        </Row>
        {count > 0 && (
          <CollapsibleContent>
            <SidebarMenuSub className={TIGHT_SUBMENU}>{children}</SidebarMenuSub>
          </CollapsibleContent>
        )}
      </SidebarMenuSubItem>
    </Collapsible>
  )
}

// A pipeline or an operator: opens the folder page on its tab.
function SetupRow({ name, onOpen }: { name: string; onOpen: () => void }) {
  return (
    <SidebarMenuSubItem>
      <Row>
        <SidebarMenuSubButton asChild>
          <button type="button" className="w-full font-mono" onClick={onOpen}>
            {name}
          </button>
        </SidebarMenuSubButton>
      </Row>
    </SidebarMenuSubItem>
  )
}

// A closed deployment, listed after the open ones: opens it to read its
// missions and log.
function PastDeploymentRow({ deployment, isSelected, onOpen }: { deployment: Deployment; isSelected: boolean; onOpen: () => void }) {
  return (
    <SidebarMenuSubItem>
      <Row>
        <div className="flex items-center">
          {/* Where an open deployment's chevron sits, so the names line up. */}
          <span className="size-4 flex-none" />
          <SidebarMenuSubButton asChild isActive={isSelected}>
            <button type="button" className="w-full text-muted-foreground" onClick={onOpen}>
              <ActivityDot />
              <span className="truncate">{deployment.name}</span>
              <span className="ml-auto flex-none font-mono text-label">{pipelineTag(deployment.pipeline)}</span>
            </button>
          </SidebarMenuSubButton>
        </div>
      </Row>
    </SidebarMenuSubItem>
  )
}

type EscalationRowProps = { escalation: Escalation; isSelected: boolean; onOpen: () => void; onDismiss: () => void }

function EscalationRow({ escalation, isSelected, onOpen, onDismiss }: EscalationRowProps) {
  const dismiss =
    escalation.kind === 'suggestion' ? (
      <Button variant="ghost" size="icon-xs" aria-label="Dismiss" title="Dismiss" onClick={onDismiss}>
        <X className="size-4" />
      </Button>
    ) : undefined
  return (
    <SidebarMenuSubItem>
      <Row action={dismiss}>
        <SidebarMenuSubButton asChild isActive={isSelected}>
          <button type="button" className="w-full" onClick={onOpen}>
            <span className="flex-none font-mono text-label text-muted-foreground">{KIND_LABELS[escalation.kind]}</span>
            <span className="truncate">{escalation.text}</span>
          </button>
        </SidebarMenuSubButton>
      </Row>
    </SidebarMenuSubItem>
  )
}

type DeploymentRowProps = {
  snapshot: DeploymentSnapshot
  escalations: readonly Escalation[]
  selectedKey?: string
  onSelect: () => void
  onOpenOperator: (position: string) => void
  onOpenEscalation: (escalation: Escalation) => void
  onDismissEscalation: (escalation: Escalation) => void
}

// An operator in the deployment: its activity, or "inactive" when its
// pipeline names it but no session is running.
function OperatorRow({ entry, onOpen }: { entry: RosterEntry; onOpen: () => void }) {
  const label = entry.session === undefined ? INACTIVE_LABEL : sessionStatus(entry.session)
  return (
    <SidebarMenuSubItem>
      <Row>
        <SidebarMenuSubButton asChild>
          <button type="button" className={cn('w-full', entry.session === undefined && 'text-muted-foreground')} onClick={onOpen} title={label}>
            <ActivityDot session={entry.session} />
            <span className="truncate font-mono">{entry.position}</span>
            <CommanderCrown position={entry.position} />
            <span className="ml-auto flex-none text-label text-muted-foreground">{label}</span>
          </button>
        </SidebarMenuSubButton>
      </Row>
    </SidebarMenuSubItem>
  )
}

// A deployment, and under it its operators (each with its activity's color)
// and what it has escalated. The chevron folds them; the name opens it.
function DeploymentRow({ snapshot, escalations, selectedKey, onSelect, onOpenOperator, onOpenEscalation, onDismissEscalation }: DeploymentRowProps) {
  const isWorking = snapshot.sessions.some(session => session.activity === 'busy')
  const roster = rosterOf(snapshot.sessions, snapshot.pipelineOperators)
  const hasChildren = roster.length > 0 || escalations.length > 0
  return (
    <Collapsible defaultOpen asChild className="group/deployment">
      <SidebarMenuSubItem>
        <Row action={<DeploymentMenu deployment={snapshot.deployment} sessions={snapshot.sessions} />}>
          <div className="flex items-center">
            {hasChildren ? (
              <CollapsibleTrigger asChild>
                <button type="button" className="flex-none" aria-label={`Show ${snapshot.deployment.name}'s operators and escalations`}>
                  <Chevron group="deployment" />
                </button>
              </CollapsibleTrigger>
            ) : (
              <span className="size-4 flex-none" />
            )}
            <SidebarMenuSubButton asChild isActive={selectedKey === `deployment:${snapshot.deployment.id}`}>
              <button type="button" className="w-full" onClick={onSelect}>
                <span className="dot flex-none" style={{ color: isWorking ? 'var(--success)' : 'var(--fg-muted)' }} />
                <span className="truncate">{snapshot.deployment.name}</span>
                {escalations.length > 0 ? (
                  <span className="count ml-auto">{escalations.length}</span>
                ) : (
                  <span className="ml-auto flex-none font-mono text-label text-muted-foreground">{pipelineTag(snapshot.deployment.pipeline)}</span>
                )}
              </button>
            </SidebarMenuSubButton>
          </div>
        </Row>
        {hasChildren && (
          <CollapsibleContent>
            <SidebarMenuSub className={TIGHT_SUBMENU}>
              {roster.map(entry => (
                <OperatorRow key={entry.position} entry={entry} onOpen={() => onOpenOperator(entry.position)} />
              ))}
              {escalations.map(escalation => (
                <EscalationRow
                  key={escalation.key}
                  escalation={escalation}
                  isSelected={selectedKey === escalation.key}
                  onOpen={() => onOpenEscalation(escalation)}
                  onDismiss={() => onDismissEscalation(escalation)}
                />
              ))}
            </SidebarMenuSub>
          </CollapsibleContent>
        )}
      </SidebarMenuSubItem>
    </Collapsible>
  )
}

export function AppSidebar(props: AppSidebarProps) {
  const { folders, snapshots, escalations, selectedKey } = props
  return (
    <Sidebar collapsible="none" className="h-full">
      <SidebarHeader>
        <NewProjectDialog onAdded={props.onFolderAdded} />
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel className="label">Local folders</SidebarGroupLabel>
          {/* 21px between folders, so each reads as its own block. */}
          <SidebarMenu className="gap-5">
            {folders.map(folder => {
              const inFolder = snapshots.filter(snapshot => snapshot.deployment.folder === folder.path)
              const deployments = inFolder.filter(snapshot => snapshot.deployment.closed_ms === null)
              const pastDeployments = inFolder
                .map(snapshot => snapshot.deployment)
                .filter(deployment => deployment.closed_ms !== null)
                .sort((first, second) => second.started_ms - first.started_ms)
              const folderEscalations = escalations.filter(escalation => escalation.folderPath === folder.path)
              const settingsButton = (
                <Button variant="ghost" size="icon-xs" aria-label={`${folder.name}'s settings and setup`} title="Settings and setup" onClick={() => props.onOpenFolder(folder)}>
                  <Settings className="size-4" />
                </Button>
              )
              return (
                <Collapsible key={folder.path} defaultOpen asChild className="group/folder">
                  <SidebarMenuItem>
                    <Row action={settingsButton}>
                      <CollapsibleTrigger asChild>
                        <SidebarMenuButton isActive={selectedKey === `folder:${folder.path}`}>
                          <Chevron group="folder" />
                          <span className="truncate font-medium">{folder.name}</span>
                          {folderEscalations.length > 0 && <span className="count ml-auto">{folderEscalations.length}</span>}
                        </SidebarMenuButton>
                      </CollapsibleTrigger>
                    </Row>
                    <CollapsibleContent>
                      <SidebarMenuSub className={TIGHT_SUBMENU}>
                        <Section label="Deployments" count={deployments.length + pastDeployments.length} action={<StartDeploymentDialog folder={folder} onStarted={props.onSelectDeployment} />}>
                          {deployments.map(snapshot => (
                            <DeploymentRow
                              key={snapshot.deployment.id}
                              snapshot={snapshot}
                              escalations={folderEscalations.filter(escalation => escalation.deploymentId === snapshot.deployment.id)}
                              selectedKey={selectedKey}
                              onSelect={() => props.onSelectDeployment(snapshot.deployment)}
                              onOpenOperator={position => props.onOpenOperator(snapshot.deployment, position)}
                              onOpenEscalation={props.onOpenEscalation}
                              onDismissEscalation={props.onDismissEscalation}
                            />
                          ))}
                          {pastDeployments.map(deployment => (
                            <PastDeploymentRow
                              key={deployment.id}
                              deployment={deployment}
                              isSelected={selectedKey === `deployment:${deployment.id}`}
                              onOpen={() => props.onSelectDeployment(deployment)}
                            />
                          ))}
                        </Section>
                        <Section label="Pipelines" count={folder.pipelines.length}>
                          {folder.pipelines.map(pipeline => (
                            <SetupRow key={pipeline} name={pipeline} onOpen={() => props.onOpenFolder(folder, 'pipelines')} />
                          ))}
                        </Section>
                        <Section label="Operators" count={folder.operators.length}>
                          {[...folder.operators].sort(byOperatorOrder).map(operator => (
                            <SetupRow key={operator} name={operator} onOpen={() => props.onOpenFolder(folder, 'operators')} />
                          ))}
                        </Section>
                      </SidebarMenuSub>
                    </CollapsibleContent>
                  </SidebarMenuItem>
                </Collapsible>
              )
            })}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </Sidebar>
  )
}
