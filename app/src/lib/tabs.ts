// The main area's tabs, as in an editor: each opened page stays open in the
// strip along the top until it's closed, and one of them shows.

// Each folder page's own tabs, in the order they show.
export const FOLDER_TABS = ['deployments', 'pipelines', 'operators', 'settings'] as const
export type FolderTab = (typeof FOLDER_TABS)[number]
export const isFolderTab = (value: unknown): value is FolderTab => FOLDER_TABS.some(tab => tab === value)

// What a deployment can open in a tab: the groups under it in the sidebar.
export const DEPLOYMENT_PARTS = ['operators', 'questions', 'decisions', 'missions', 'templates'] as const
export type DeploymentPart = (typeof DEPLOYMENT_PARTS)[number]
export const isDeploymentPart = (value: unknown): value is DeploymentPart => DEPLOYMENT_PARTS.some(part => part === value)

export const PART_LABELS: Record<DeploymentPart, string> = {
  operators: 'Operators',
  questions: 'Questions',
  decisions: 'Decisions',
  missions: 'Missions',
  templates: 'Mission templates',
}

export type Tab =
  | { kind: 'deployment'; deploymentId: string; part: DeploymentPart }
  | { kind: 'folder'; path: string; folderTab: FolderTab }
  // Everything, in every folder, that stops until you act.
  | { kind: 'blockers' }
  // The channels: their state, the teams on them and their log.
  | { kind: 'channels' }

export const BLOCKERS_KEY = 'blockers'
export const CHANNELS_KEY = 'channels'

export type OpenTabs = { tabs: readonly Tab[]; activeKey?: string }

export const NO_TABS: OpenTabs = { tabs: [] }

// One tab per page: a folder's page is one tab whichever of its own tabs shows.
export const tabKey = (tab: Tab): string => {
  switch (tab.kind) {
    case 'deployment':
      return `deployment:${tab.deploymentId}:${tab.part}`
    case 'folder':
      return `folder:${tab.path}`
    case 'blockers':
      return BLOCKERS_KEY
    case 'channels':
      return CHANNELS_KEY
  }
}

export const activeTab = ({ tabs, activeKey }: OpenTabs): Tab | undefined => tabs.find(tab => tabKey(tab) === activeKey)

// Shows the tab: a page already open is brought forward (taking the new
// tab's details, such as a folder page's own tab); a new one goes on the end.
export const openTab = (open: OpenTabs, tab: Tab): OpenTabs => {
  const key = tabKey(tab)
  const isOpen = open.tabs.some(openTab => tabKey(openTab) === key)
  const tabs = isOpen ? open.tabs.map(openTab => (tabKey(openTab) === key ? tab : openTab)) : [...open.tabs, tab]
  return { tabs, activeKey: key }
}

// Closing the tab that shows moves to its right-hand neighbour, or its left
// when it was the last.
export const closeTab = (open: OpenTabs, key: string): OpenTabs => {
  const index = open.tabs.findIndex(tab => tabKey(tab) === key)
  if (index === -1) return open
  const tabs = open.tabs.filter(tab => tabKey(tab) !== key)
  if (open.activeKey !== key) return { tabs, activeKey: open.activeKey }
  const neighbour = tabs[Math.min(index, tabs.length - 1)]
  return { tabs, activeKey: neighbour === undefined ? undefined : tabKey(neighbour) }
}

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null

const parseTab = (value: unknown): Tab | undefined => {
  if (!isRecord(value)) return undefined
  if (value.kind === 'deployment' && typeof value.deploymentId === 'string' && isDeploymentPart(value.part)) {
    return { kind: 'deployment', deploymentId: value.deploymentId, part: value.part }
  }
  if (value.kind === 'blockers') return { kind: 'blockers' }
  if (value.kind === 'channels') return { kind: 'channels' }
  if (value.kind === 'folder' && typeof value.path === 'string') {
    return { kind: 'folder', path: value.path, folderTab: isFolderTab(value.folderTab) ? value.folderTab : FOLDER_TABS[0] }
  }
  return undefined
}

// Saved tabs, keeping the ones that can be read; none when nothing can be.
export const parseOpenTabs = (saved: string | null): OpenTabs => {
  if (saved === null) return NO_TABS
  try {
    const value: unknown = JSON.parse(saved)
    if (!isRecord(value) || !Array.isArray(value.tabs)) return NO_TABS
    const tabs = value.tabs.flatMap(tab => parseTab(tab) ?? [])
    const activeKey = tabs.map(tabKey).find(key => key === value.activeKey) ?? (tabs[0] === undefined ? undefined : tabKey(tabs[0]))
    return { tabs, activeKey }
  } catch {
    return NO_TABS
  }
}

// The sidebar row to mark for the tab that shows.
export const selectedRowKey = (tab: Tab | undefined): string | undefined => {
  if (tab === undefined) return undefined
  switch (tab.kind) {
    case 'deployment':
      return `deployment:${tab.deploymentId}`
    case 'folder':
      return `folder:${tab.path}`
    case 'blockers':
      return BLOCKERS_KEY
    case 'channels':
      return CHANNELS_KEY
  }
}
