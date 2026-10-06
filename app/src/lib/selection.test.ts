import { describe, expect, it } from 'vitest'

import { mainView, selectedRowKey } from './selection'

describe('mainView', () => {
  it('shows a question first, then a folder, then a deployment', () => {
    expect(mainView({ questionKey: 'question:5', folderPath: '/repo', deploymentId: 'd1' })).toEqual({ kind: 'question', key: 'question:5' })
    expect(mainView({ folderPath: '/repo', deploymentId: 'd1' })).toEqual({ kind: 'folder', path: '/repo', tab: 'deployments' })
    expect(mainView({ folderPath: '/repo', folderTab: 'settings' })).toEqual({ kind: 'folder', path: '/repo', tab: 'settings' })
    expect(mainView({ deploymentId: 'd1' })).toEqual({ kind: 'deployment', id: 'd1' })
    expect(mainView({})).toEqual({ kind: 'nothing' })
  })
})

describe('selectedRowKey', () => {
  it('matches the sidebar row keys', () => {
    expect(selectedRowKey({ kind: 'folder', path: '/repo', tab: 'pipelines' })).toBe('folder:/repo')
    expect(selectedRowKey({ kind: 'deployment', id: 'd1' })).toBe('deployment:d1')
    expect(selectedRowKey({ kind: 'question', key: 'question:5' })).toBe('question:5')
    expect(selectedRowKey({ kind: 'nothing' })).toBeUndefined()
  })
})
