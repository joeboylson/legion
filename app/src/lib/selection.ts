// What the main area shows, and which sidebar row is marked as chosen.

// Each page's tabs, in the order they show.
export const FOLDER_TABS = ['deployments', 'pipelines', 'operators', 'settings'] as const
export type FolderTab = (typeof FOLDER_TABS)[number]
export const isFolderTab = (value: string | undefined): value is FolderTab => FOLDER_TABS.some(tab => tab === value)

export const DEPLOYMENT_TABS = ['missions', 'log', 'operators'] as const
export type DeploymentTab = (typeof DEPLOYMENT_TABS)[number]
export const isDeploymentTab = (value: string | undefined): value is DeploymentTab => DEPLOYMENT_TABS.some(tab => tab === value)

export type MainView =
  | { kind: 'question'; key: string }
  | { kind: 'folder'; path: string; tab: FolderTab }
  | { kind: 'deployment'; id: string }
  | { kind: 'nothing' }

export type Selection = {
  questionKey?: string
  folderPath?: string
  folderTab?: FolderTab
  deploymentId?: string
}

// A question in front of a folder in front of a deployment.
export const mainView = (selection: Selection): MainView => {
  if (selection.questionKey !== undefined) return { kind: 'question', key: selection.questionKey }
  if (selection.folderPath !== undefined) return { kind: 'folder', path: selection.folderPath, tab: selection.folderTab ?? 'deployments' }
  if (selection.deploymentId !== undefined) return { kind: 'deployment', id: selection.deploymentId }
  return { kind: 'nothing' }
}

// The sidebar row for a view, keyed the way SidebarTree keys its rows.
export const selectedRowKey = (view: MainView): string | undefined => {
  switch (view.kind) {
    case 'question':
      return view.key
    case 'folder':
      return `folder:${view.path}`
    case 'deployment':
      return `deployment:${view.id}`
    case 'nothing':
      return undefined
  }
}
