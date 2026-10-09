// What needs the admin before work can go on, and where the sidebar shows
// it: on the deepest row that can be seen, so a closed folder carries the
// mark for an operator inside it, and an open tree carries it on the
// operator itself.

import type { Escalation, EscalationKind } from '@/lib/escalations'

// The work stops until you act: a session that won't start, a tool waiting
// for permission, or a session stopped in its terminal.
export const BLOCKER_KINDS: readonly EscalationKind[] = ['stuck', 'permission', 'halted']

// The kinds about one operator's session, shown with the operators rather
// than with the questions.
export const OPERATOR_KINDS: readonly EscalationKind[] = ['stuck', 'permission', 'halted', 'limit']

export const blockersIn = (items: readonly Escalation[]): Escalation[] => items.filter(item => BLOCKER_KINDS.includes(item.kind))

// The keys the sidebar opens its rows by (see AppSidebar).
export const rowKeys = {
  folder: (path: string) => `folder:${path}`,
  folderSection: (path: string, section: string) => `folder:${path}:${section}`,
  deployment: (deploymentId: string) => `deployment:${deploymentId}`,
  deploymentPart: (deploymentId: string, part: string) => `deployment:${deploymentId}:${part}`,
  operator: (deploymentId: string, position: string) => `operator:${deploymentId}:${position}`,
} as const

export const DEPLOYMENTS_SECTION = 'deployments'
export const OPERATORS_PART = 'assigned operators'

// The rows from the folder down to the item's own, outermost first.
const rowPath = (item: Escalation): string[] => [
  rowKeys.folder(item.folderPath),
  rowKeys.folderSection(item.folderPath, DEPLOYMENTS_SECTION),
  rowKeys.deployment(item.deploymentId),
  rowKeys.deploymentPart(item.deploymentId, OPERATORS_PART),
  rowKeys.operator(item.deploymentId, item.position ?? ''),
]

// The first row on the way down that's closed, or the item's own row when
// every one above it is open.
export const markedRow = (item: Escalation, isOpen: (key: string) => boolean): string => {
  const path = rowPath(item)
  return path.find((key, index) => index === path.length - 1 || !isOpen(key)) ?? rowKeys.folder(item.folderPath)
}

// Each marked row and the items it carries.
export const markedRows = (items: readonly Escalation[], isOpen: (key: string) => boolean): ReadonlyMap<string, readonly Escalation[]> =>
  blockersIn(items).reduce((marks, item) => {
    const key = markedRow(item, isOpen)
    return new Map([...marks, [key, [...(marks.get(key) ?? []), item]]])
  }, new Map<string, readonly Escalation[]>())
