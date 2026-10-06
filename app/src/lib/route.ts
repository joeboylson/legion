// The view, written into the address after `#`, so a reload (or a window
// opened at that address) comes back to the same place.

import { type DeploymentTab, FOLDER_TABS, isDeploymentTab, isFolderTab, type Selection } from '@/lib/selection'

export type Route = { selection: Selection; deploymentTab: DeploymentTab }

const DEFAULT_DEPLOYMENT_TAB: DeploymentTab = 'missions'

// Folder paths and question keys hold slashes and colons, so each part is encoded.
export const routeToHash = ({ selection, deploymentTab }: Route): string => {
  if (selection.questionKey !== undefined) return `#/question/${encodeURIComponent(selection.questionKey)}`
  if (selection.folderPath !== undefined) return `#/folder/${encodeURIComponent(selection.folderPath)}/${selection.folderTab ?? FOLDER_TABS[0]}`
  if (selection.deploymentId !== undefined) return `#/deployment/${encodeURIComponent(selection.deploymentId)}/${deploymentTab}`
  return ''
}

// Anything it can't read leaves the view at its start.
export const hashToRoute = (hash: string): Route => {
  const [kind, encoded, tab] = hash.replace(/^#\/?/, '').split('/')
  const value = encoded === undefined ? undefined : decodeURIComponent(encoded)
  const start: Route = { selection: {}, deploymentTab: DEFAULT_DEPLOYMENT_TAB }
  if (value === undefined || value === '') return start
  switch (kind) {
    case 'question':
      return { ...start, selection: { questionKey: value } }
    case 'folder':
      return { ...start, selection: { folderPath: value, folderTab: isFolderTab(tab) ? tab : undefined } }
    case 'deployment':
      return { selection: { deploymentId: value }, deploymentTab: isDeploymentTab(tab) ? tab : DEFAULT_DEPLOYMENT_TAB }
    default:
      return start
  }
}
