// The sidebar, built from shadcn's Sidebar: each local folder, and under it
// its deployments, pipelines and operators. Each deployment holds its active
// operators, questions, decisions, missions and mission templates. Every
// level folds like a file tree. A blocker is marked on the deepest row you
// can see, and every one is listed together on the Blockers page.

import { ChevronRight, Search, SquareArrowOutUpRight, TriangleAlert, X } from 'lucide-react'
import { createContext, type ReactNode, useContext, useMemo } from 'react'

import { ActivityDot } from '@/components/ActivityDot'
import { CommanderCrown } from '@/components/CommanderCrown'
import { ActionContextMenu, ActionMenuButton, AddButton, type MenuOpen } from '@/components/actions/ActionMenus'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from '@/components/ui/sidebar'
import type { Channels } from '@/generated/Channels'
import type { Deployment } from '@/generated/Deployment'
import type { Folder } from '@/generated/Folder'
import type { Mission } from '@/generated/Mission'
import { useActions } from '@/hooks/useActions'
import { OpenRowsContext, useOpenRowsState, useRowOpen } from '@/hooks/useOpenRows'
import { DEPLOYMENTS_SECTION, markedRows, blockersIn, BLOCKER_KINDS, OPERATOR_KINDS, OPERATORS_PART, rowKeys } from '@/lib/blockers'
import { DISMISSIBLE_KINDS, type DeploymentSnapshot, type Escalation, type EscalationKind, KIND_LABELS } from '@/lib/escalations'
import { INACTIVE_LABEL, isWorking, MISSION_STATUS_LABELS, pipelineLabel, pipelineTag, sessionStatus, workingDotColor } from '@/lib/format'
import { byOperatorOrder, type RosterEntry, rosterOf } from '@/lib/roster'
import { BLOCKERS_KEY, CHANNELS_KEY, type DeploymentPart, type FolderTab } from '@/lib/tabs'
import { cn } from '@/lib/utils'

// The border round each folder and the Blockers button, each a block of its
// own: the muted text color at half strength, brighter than the usual line.
const BOXED = 'rounded-lg border p-1'
const BOX_BORDER = 'border-[color-mix(in_srgb,var(--fg-muted)_50%,transparent)]'

// A little room between items (3px), and every level reaches the same right
// edge so the action column lines up.
const TIGHT_SUBMENU = 'mr-0 translate-x-0 gap-1 pt-1 pb-0 pr-0'

// Green while every channel is up, red when one is down, grey with none.
const channelsDotColor = (channels: Channels | undefined): string => {
  const ends = [
    ...(channels?.hosted === null || channels?.hosted === undefined ? [] : [channels.hosted.problem === null]),
    ...(channels?.subscriptions.map(subscription => subscription.is_up) ?? []),
  ]
  if (ends.length === 0) return 'var(--fg-muted)'
  return ends.every(Boolean) ? 'var(--success)' : 'var(--danger)'
}

type AppSidebarProps = {
  folders: readonly Folder[]
  snapshots: readonly DeploymentSnapshot[]
  escalations: readonly Escalation[]
  channels?: Channels
  selectedKey?: string
  onOpenBlockers: () => void
  onOpenChannels: () => void
  onOpenFolder: (folder: Folder, tab?: FolderTab) => void
  onSelectDeployment: (deployment: Deployment) => void
  onOpenOperator: (deployment: Deployment, position: string) => void
  onOpenPart: (deployment: Deployment, part: DeploymentPart) => void
  onOpenMission: (deployment: Deployment, mission: Mission) => void
  onOpenEscalation: (escalation: Escalation) => void
  onDismissEscalation: (escalation: Escalation) => void
}

// The chevron turns as its Collapsible (named by `group`) opens.
// Spelled out: Tailwind only builds classes it can find written whole.
const TURNS_WHEN_OPEN = {
  folder: 'group-data-[state=open]/folder:rotate-90',
  section: 'group-data-[state=open]/section:rotate-90',
  deployment: 'group-data-[state=open]/deployment:rotate-90',
  part: 'group-data-[state=open]/part:rotate-90',
} as const

// A section's own group name, so a chevron turns only with its own section:
// a deployment's parts sit inside the folder's Deployments section.
const SECTION_GROUPS = {
  section: 'group/section',
  part: 'group/part',
} as const

function Chevron({ group }: { group: keyof typeof TURNS_WHEN_OPEN }) {
  return <ChevronRight className={`size-4 flex-none transition-transform ${TURNS_WHEN_OPEN[group]}`} />
}

// Each marked row and what it carries, and where its mark leads.
type Marks = { rows: ReadonlyMap<string, readonly Escalation[]>; onOpen: () => void }
const MarksContext = createContext<Marks>({ rows: new Map(), onOpen: () => undefined })

// The blockers under this row, while the row hides them (or its own).
// Click it for the Blockers page.
function BlockerMark({ rowKey }: { rowKey: string }) {
  const marks = useContext(MarksContext)
  const items = marks.rows.get(rowKey) ?? []
  if (items.length === 0) return null
  const list = items.map(item => `${KIND_LABELS[item.kind]} · ${item.position ?? ''}: ${item.text}`).join('\n')
  return (
    <button type="button" aria-label={`${items.length} blocked`} title={list} className="flex-none" onClick={marks.onOpen}>
      <AlertMark count={items.length} />
    </button>
  )
}

// A warning sign with its count in a red bubble on its corner, as on a
// notification. The bubble sits above the rows around it, and its parent
// must not clip (a menu button's last span is truncated, so it goes after
// the button, not in it).
function AlertMark({ count }: { count: number }) {
  return (
    <span className="relative flex size-5 items-center justify-center text-warning">
      <TriangleAlert className="size-4" />
      <Badge
        variant="destructive"
        className="absolute -top-1.5 -right-2 z-10 h-4 min-w-4 px-1 py-0 font-mono leading-none tabular-nums dark:bg-destructive"
      >
        {count}
      </Badge>
    </span>
  )
}

// Every row: its button, its mark if anything under it is blocked, then one
// fixed-width slot for an action (or nothing), so actions and counts line up
// down the whole tree.
function Row({ rowKey, children, action }: { rowKey?: string; children: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex items-center gap-1">
      <div className="min-w-0 flex-1">{children}</div>
      {rowKey !== undefined && <BlockerMark rowKey={rowKey} />}
      <span className="flex size-5 flex-none items-center justify-center">{action}</span>
    </div>
  )
}

// Marks a row with nothing under it, set apart from the rows that fold.
function LeafMark() {
  return (
    <span aria-hidden className="flex-none text-muted-foreground">
      —
    </span>
  )
}

function Count({ value }: { value: number }) {
  return <span className="ml-auto flex-none font-mono text-label text-muted-foreground">{value}</span>
}

type SectionProps = { rowKey: string; label: string; count: number; group?: keyof typeof SECTION_GROUPS; action?: ReactNode; children?: ReactNode }

// A group under a folder (deployments, pipelines …) or a deployment
// (questions, missions …): a row that folds, and its items.
function Section({ rowKey, label, count, group = 'section', action, children }: SectionProps) {
  const openProps = useRowOpen(rowKey)
  return (
    <Collapsible asChild className={SECTION_GROUPS[group]} {...openProps}>
      <SidebarMenuSubItem>
        <Row rowKey={rowKey} action={action}>
          <CollapsibleTrigger asChild>
            <SidebarMenuSubButton asChild>
              <button type="button" className="w-full">
                <Chevron group={group} />
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
function SetupRow({ name, folder, subject, onOpen }: { name: string; folder: string; subject: 'pipeline' | 'operator'; onOpen: () => void }) {
  return (
    <ActionContextMenu subject={subject} prefill={{ folder, [subject]: name }} open={{ label: 'Open', onOpen }}>
      <SidebarMenuSubItem>
        <Row>
          <SidebarMenuSubButton asChild>
            <button type="button" className="w-full font-mono" onClick={onOpen}>
              <LeafMark />
              {subject === 'pipeline' ? pipelineLabel(name) : name}
            </button>
          </SidebarMenuSubButton>
        </Row>
      </SidebarMenuSubItem>
    </ActionContextMenu>
  )
}

// Their section already says what they are; the rest of Questions are mixed.
const UNLABELLED_KINDS: readonly EscalationKind[] = ['question', 'decision']

type EscalationRowProps = { escalation: Escalation; isSelected: boolean; onOpen: () => void; onDismiss: () => void }

// A question can be answered from its menu; the rest only opened.
const escalationPrefill = (escalation: Escalation) =>
  escalation.questionId === undefined ? { deployment: escalation.deploymentId } : { deployment: escalation.deploymentId, question: String(escalation.questionId) }

function EscalationRow({ escalation, isSelected, onOpen, onDismiss }: EscalationRowProps) {
  const dismiss =
    DISMISSIBLE_KINDS.includes(escalation.kind) ? (
      <Button variant="ghost" size="icon-xs" aria-label="Dismiss" title="Dismiss" onClick={onDismiss}>
        <X className="size-4" />
      </Button>
    ) : undefined
  return (
    <ActionContextMenu subject={escalation.kind === 'question' ? 'question' : 'deployment'} prefill={escalationPrefill(escalation)} open={{ label: 'Open', onOpen }}>
      <SidebarMenuSubItem>
        <Row action={dismiss}>
          <SidebarMenuSubButton asChild isActive={isSelected}>
            <button type="button" className="w-full" onClick={onOpen}>
              <LeafMark />
              {!UNLABELLED_KINDS.includes(escalation.kind) && <span className="flex-none font-mono text-label text-muted-foreground">{KIND_LABELS[escalation.kind]}</span>}
              <span className="truncate">{escalation.text}</span>
            </button>
          </SidebarMenuSubButton>
        </Row>
      </SidebarMenuSubItem>
    </ActionContextMenu>
  )
}

type DeploymentRowProps = {
  snapshot: DeploymentSnapshot
  escalations: readonly Escalation[]
  selectedKey?: string
  onOpenOperator: (position: string) => void
  onOpenPart: (part: DeploymentPart) => void
  onOpenMission: (mission: Mission) => void
  onOpenEscalation: (escalation: Escalation) => void
  onDismissEscalation: (escalation: Escalation) => void
}

// An operator in the deployment: its activity, or "inactive" when its
// pipeline names it but no session is running.
function OperatorRow({ deploymentId, entry, onOpen }: { deploymentId: string; entry: RosterEntry; onOpen: () => void }) {
  const label = entry.session === undefined ? INACTIVE_LABEL : sessionStatus(entry.session)
  // A running copy is a session to stop or talk to; an idle operator, one to start.
  const prefill = entry.session === undefined ? { deployment: deploymentId, operator: entry.position } : { deployment: deploymentId, position: entry.position }
  return (
    <ActionContextMenu subject="session" prefill={prefill} open={{ label: 'Open its terminal', onOpen }}>
      <SidebarMenuSubItem>
        <Row rowKey={rowKeys.operator(deploymentId, entry.position)}>
          <SidebarMenuSubButton asChild>
            <button type="button" className={cn('w-full', entry.session === undefined && 'text-muted-foreground')} onClick={onOpen} title={label}>
              <LeafMark />
              <ActivityDot session={entry.session} />
              <span className="truncate font-mono">{entry.position}</span>
              <CommanderCrown position={entry.position} />
              <span className="ml-auto flex-none text-label text-muted-foreground">{label}</span>
            </button>
          </SidebarMenuSubButton>
        </Row>
      </SidebarMenuSubItem>
    </ActionContextMenu>
  )
}

// Opens one of a deployment's parts in its own tab.
function OpenTabButton({ label, onOpen }: { label: string; onOpen: () => void }) {
  return (
    <Button variant="ghost" size="icon-xs" aria-label={`Open ${label} in a tab`} title="Open in a tab" onClick={onOpen}>
      <SquareArrowOutUpRight className="size-4" />
    </Button>
  )
}

// A mission in the deployment: opens the deployment's missions in a tab,
// and the mission over it.
function MissionRow({ deploymentId, mission, onOpen }: { deploymentId: string; mission: Mission; onOpen: () => void }) {
  const status = MISSION_STATUS_LABELS[mission.status]
  return (
    <ActionContextMenu subject="mission" prefill={{ deployment: deploymentId, mission: String(mission.number) }} open={{ label: 'Read it', onOpen }}>
      <SidebarMenuSubItem>
        <Row>
          <SidebarMenuSubButton asChild>
            <button type="button" className={cn('w-full', mission.status === 'done' && 'text-muted-foreground')} onClick={onOpen} title={mission.title}>
              <LeafMark />
              <span className="flex-none font-mono text-label text-muted-foreground">{mission.number}</span>
              <span className="truncate">{mission.title}</span>
              <span className="ml-auto flex-none text-label text-muted-foreground">{status}</span>
            </button>
          </SidebarMenuSubButton>
        </Row>
      </SidebarMenuSubItem>
    </ActionContextMenu>
  )
}

type DeploymentContextMenuProps = { isClosed: boolean; prefill: { deployment: string; folder: string }; open: MenuOpen; children: ReactNode }

// A closed deployment can only be reopened or deleted.
function DeploymentContextMenu({ isClosed, prefill, open, children }: DeploymentContextMenuProps) {
  return (
    <ActionContextMenu subject={isClosed ? 'closedDeployment' : 'deployment'} prefill={prefill} open={open}>
      <div>{children}</div>
    </ActionContextMenu>
  )
}

// A deployment, and under it its parts: assigned operators (each with its
// activity's color), questions, decisions, missions and mission templates.
// The whole row folds them. A closed one is greyed, with
// nothing to start or stop.
function DeploymentRow(props: DeploymentRowProps) {
  const { snapshot, escalations, selectedKey, onOpenOperator, onOpenPart, onOpenMission, onOpenEscalation, onDismissEscalation } = props
  const openProps = useRowOpen(rowKeys.deployment(snapshot.deployment.id))
  const partKey = (part: string) => rowKeys.deploymentPart(snapshot.deployment.id, part)
  const isActive = isWorking(snapshot.sessions)
  const isClosed = snapshot.deployment.closed_ms !== null
  const roster = rosterOf(snapshot.sessions, snapshot.pipelineOperators)
  // Decisions don't hold anyone up; a session's state shows on its operator.
  const decisions = escalations.filter(escalation => escalation.kind === 'decision')
  const questions = escalations.filter(escalation => escalation.kind !== 'decision' && !OPERATOR_KINDS.includes(escalation.kind))
  const waitingCount = escalations.filter(escalation => !BLOCKER_KINDS.includes(escalation.kind)).length
  const escalationRow = (escalation: Escalation) => (
    <EscalationRow
      key={escalation.key}
      escalation={escalation}
      isSelected={selectedKey === escalation.key}
      onOpen={() => onOpenEscalation(escalation)}
      onDismiss={() => onDismissEscalation(escalation)}
    />
  )
  const deploymentPrefill = { deployment: snapshot.deployment.id, folder: snapshot.deployment.folder }
  const openDeployment = { label: 'Open', onOpen: () => onOpenPart('operators') }
  const menuButton = <ActionMenuButton subject={isClosed ? 'closedDeployment' : 'deployment'} prefill={deploymentPrefill} open={openDeployment} label={snapshot.deployment.name} />
  return (
    <Collapsible asChild className="group/deployment" {...openProps}>
      <SidebarMenuSubItem>
        <DeploymentContextMenu isClosed={isClosed} prefill={deploymentPrefill} open={openDeployment}>
          <Row rowKey={rowKeys.deployment(snapshot.deployment.id)} action={menuButton}>
            <CollapsibleTrigger asChild>
              <SidebarMenuSubButton asChild isActive={selectedKey === `deployment:${snapshot.deployment.id}`}>
                <button type="button" className={cn('w-full', isClosed && 'text-muted-foreground')}>
                  <Chevron group="deployment" />
                  {isClosed ? <ActivityDot /> : <span className="dot flex-none" style={{ color: workingDotColor(isActive) }} />}
                  <span className="truncate">{snapshot.deployment.name}</span>
                  {waitingCount > 0 ? (
                    <span className="count ml-auto">{waitingCount}</span>
                  ) : (
                    <span className="ml-auto flex-none font-mono text-label text-muted-foreground">{pipelineTag(snapshot.deployment.pipeline)}</span>
                  )}
                </button>
              </SidebarMenuSubButton>
            </CollapsibleTrigger>
          </Row>
        </DeploymentContextMenu>
        <CollapsibleContent>
          <SidebarMenuSub className={TIGHT_SUBMENU}>
            <Section group="part" rowKey={partKey(OPERATORS_PART)} label="Assigned operators" action={<OpenTabButton label="assigned operators" onOpen={() => onOpenPart('operators')} />} count={roster.length}>
              {roster.map(entry => (
                <OperatorRow key={entry.position} deploymentId={snapshot.deployment.id} entry={entry} onOpen={() => onOpenOperator(entry.position)} />
              ))}
            </Section>
            <Section group="part" rowKey={partKey('questions')} label="Questions" action={<OpenTabButton label="questions" onOpen={() => onOpenPart('questions')} />} count={questions.length}>
              {questions.map(escalationRow)}
            </Section>
            <Section group="part" rowKey={partKey('decisions')} label="Decisions" action={<OpenTabButton label="decisions" onOpen={() => onOpenPart('decisions')} />} count={decisions.length}>
              {decisions.map(escalationRow)}
            </Section>
            <Section group="part" rowKey={partKey('missions')} label="Missions" action={<OpenTabButton label="missions" onOpen={() => onOpenPart('missions')} />} count={snapshot.missions.length}>
              {snapshot.missions.map(mission => (
                <MissionRow key={mission.number} deploymentId={snapshot.deployment.id} mission={mission} onOpen={() => onOpenMission(mission)} />
              ))}
            </Section>
            {/* Filled in once mission templates exist. */}
            <Section group="part" rowKey={partKey('mission templates')} label="Mission templates" action={<OpenTabButton label="mission templates" onOpen={() => onOpenPart('templates')} />} count={0} />
          </SidebarMenuSub>
        </CollapsibleContent>
      </SidebarMenuSubItem>
    </Collapsible>
  )
}

export function AppSidebar(props: AppSidebarProps) {
  const { folders, snapshots, escalations, selectedKey } = props
  const openRows = useOpenRowsState()
  const { onOpenBlockers } = props
  const marks = useMemo<Marks>(() => ({ rows: markedRows(escalations, openRows.isOpen), onOpen: onOpenBlockers }), [escalations, openRows.isOpen, onOpenBlockers])
  const blockerCount = blockersIn(escalations).length
  const { openMenu } = useActions()
  return (
    <OpenRowsContext value={openRows}>
      <MarksContext value={marks}>
        <Sidebar collapsible="none" className="h-full">
          <SidebarContent>
            <SidebarGroup className="pb-0">
              <SidebarMenu>
                <SidebarMenuItem className={cn(BOXED, BOX_BORDER)}>
                  <SidebarMenuButton onClick={openMenu} title="Go to anything, or do something">
                    <Search className="size-4 flex-none" />
                    <span className="text-muted-foreground">Go to or do…</span>
                    <kbd className="ml-auto font-mono text-label text-muted-foreground">⌘K</kbd>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroup>
            <SidebarGroup>
              <SidebarMenu>
                <SidebarMenuItem className={cn(BOXED, blockerCount > 0 ? 'border-[var(--yellow)]' : BOX_BORDER)}>
                  <Row
                    action={
                      blockerCount > 0 && (
                        <button type="button" aria-label={`${blockerCount} blocked`} onClick={props.onOpenBlockers}>
                          <AlertMark count={blockerCount} />
                        </button>
                      )
                    }
                  >
                    <SidebarMenuButton isActive={selectedKey === BLOCKERS_KEY} onClick={props.onOpenBlockers}>
                      <span className="font-medium">Blockers</span>
                    </SidebarMenuButton>
                  </Row>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroup>
            <SidebarGroup>
              {/* Inset like a folder's border, so the plus lines up with the folders' buttons. */}
              <div className="border border-transparent px-1">
                <Row action={<AddButton actionId="folder.add" prefill={{}} />}>
                  <SidebarGroupLabel className="label">Local folders</SidebarGroupLabel>
                </Row>
              </div>
              <SidebarMenu>
                {folders.map(folder => {
                  const inFolder = snapshots.filter(snapshot => snapshot.deployment.folder === folder.path)
                  const deployments = inFolder.filter(snapshot => snapshot.deployment.closed_ms === null)
                  // Closed ones after the open ones, newest first.
                  const pastDeployments = inFolder
                    .filter(snapshot => snapshot.deployment.closed_ms !== null)
                    .sort((first, second) => second.deployment.started_ms - first.deployment.started_ms)
                  const folderEscalations = escalations.filter(escalation => escalation.folderPath === folder.path)
                  // What waits on you now, not what closed deployments left; what
                  // stops the work has its own mark.
                  const waitingCount = folderEscalations.filter(escalation => !escalation.isClosed && !BLOCKER_KINDS.includes(escalation.kind)).length
                  const isFolderActive = deployments.some(snapshot => isWorking(snapshot.sessions))
                  const openFolder = { label: 'Open settings and setup', onOpen: () => props.onOpenFolder(folder) }
                  const folderMenu = <ActionMenuButton subject="folder" prefill={{ folder: folder.path }} open={openFolder} label={folder.name} />
                  return (
                    <Collapsible
                      key={folder.path}
                      asChild
                      className="group/folder"
                      open={openRows.isOpen(rowKeys.folder(folder.path))}
                      onOpenChange={isOpen => openRows.setOpen(rowKeys.folder(folder.path), isOpen)}
                    >
                      {/* A border round each folder, so its whole tree reads as one block. */}
                      <SidebarMenuItem className={cn(BOXED, BOX_BORDER)}>
                        <ActionContextMenu subject="folder" prefill={{ folder: folder.path }} open={openFolder}>
                          <div>
                            <Row rowKey={rowKeys.folder(folder.path)} action={folderMenu}>
                              <CollapsibleTrigger asChild>
                                <SidebarMenuButton isActive={selectedKey === `folder:${folder.path}`}>
                                  <Chevron group="folder" />
                                  <span className="dot flex-none" style={{ color: workingDotColor(isFolderActive) }} />
                                  <span className="truncate font-medium">{folder.name}</span>
                                  {waitingCount > 0 && <span className="count ml-auto">{waitingCount}</span>}
                                </SidebarMenuButton>
                              </CollapsibleTrigger>
                            </Row>
                          </div>
                        </ActionContextMenu>
                        <CollapsibleContent>
                          <SidebarMenuSub className={TIGHT_SUBMENU}>
                            <Section rowKey={rowKeys.folderSection(folder.path, DEPLOYMENTS_SECTION)} label="Deployments" count={deployments.length + pastDeployments.length} action={<AddButton actionId="deployment.start" prefill={{ folder: folder.path }} />}>
                              {[...deployments, ...pastDeployments].map(snapshot => (
                                <DeploymentRow
                                  key={snapshot.deployment.id}
                                  snapshot={snapshot}
                                  escalations={folderEscalations.filter(escalation => escalation.deploymentId === snapshot.deployment.id)}
                                  selectedKey={selectedKey}
                                  onOpenOperator={position => props.onOpenOperator(snapshot.deployment, position)}
                                  onOpenPart={part => props.onOpenPart(snapshot.deployment, part)}
                                  onOpenMission={mission => props.onOpenMission(snapshot.deployment, mission)}
                                  onOpenEscalation={props.onOpenEscalation}
                                  onDismissEscalation={props.onDismissEscalation}
                                />
                              ))}
                            </Section>
                            <Section rowKey={rowKeys.folderSection(folder.path, 'pipelines')} label="Pipelines" count={folder.pipelines.length} action={<AddButton actionId="pipeline.add" prefill={{ folder: folder.path }} />}>
                              {folder.pipelines.map(pipeline => (
                                <SetupRow key={pipeline} name={pipeline} folder={folder.path} subject="pipeline" onOpen={() => props.onOpenFolder(folder, 'pipelines')} />
                              ))}
                            </Section>
                            <Section rowKey={rowKeys.folderSection(folder.path, 'operators')} label="Operators" count={folder.operators.length} action={<AddButton actionId="operator.add" prefill={{ folder: folder.path }} />}>
                              {[...folder.operators].sort(byOperatorOrder).map(operator => (
                                <SetupRow key={operator} name={operator} folder={folder.path} subject="operator" onOpen={() => props.onOpenFolder(folder, 'operators')} />
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
            <SidebarGroup className="mt-auto">
              <SidebarMenu>
                <ActionContextMenu subject="channel" prefill={{}} open={{ label: 'Open', onOpen: props.onOpenChannels }}>
                  <SidebarMenuItem className={cn(BOXED, BOX_BORDER)}>
                    <SidebarMenuButton isActive={selectedKey === CHANNELS_KEY} onClick={props.onOpenChannels}>
                      <span className="dot flex-none" style={{ color: channelsDotColor(props.channels) }} />
                      <span className="font-medium">Channels</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </ActionContextMenu>
              </SidebarMenu>
            </SidebarGroup>
          </SidebarContent>
        </Sidebar>
      </MarksContext>
    </OpenRowsContext>
  )
}
