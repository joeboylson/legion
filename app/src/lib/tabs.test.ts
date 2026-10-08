import { describe, expect, it } from 'vitest'

import { activeTab, closeTab, NO_TABS, openTab, type OpenTabs, parseOpenTabs, selectedRowKey, type Tab, tabKey } from './tabs'

const operators: Tab = { kind: 'deployment', deploymentId: 'd1', part: 'operators' }
const missions: Tab = { kind: 'deployment', deploymentId: 'd1', part: 'missions' }
const folder: Tab = { kind: 'folder', path: '/repo', folderTab: 'deployments' }

const threeOpen = [operators, missions, folder].reduce(openTab, NO_TABS)

describe('opening tabs', () => {
  it('adds a new page on the end and shows it', () => {
    expect(threeOpen.tabs).toEqual([operators, missions, folder])
    expect(activeTab(threeOpen)).toEqual(folder)
  })

  it('brings an open page forward instead of opening it twice', () => {
    const settings: Tab = { kind: 'folder', path: '/repo', folderTab: 'settings' }
    const reopened = openTab(openTab(threeOpen, operators), settings)
    expect(reopened.tabs).toEqual([operators, missions, settings])
    expect(activeTab(reopened)).toEqual(settings)
  })
})

describe('closing tabs', () => {
  const showingMissions: OpenTabs = { ...threeOpen, activeKey: tabKey(missions) }

  it('moves to the right-hand neighbour, or the left one at the end', () => {
    expect(activeTab(closeTab(showingMissions, tabKey(missions)))).toEqual(folder)
    expect(activeTab(closeTab(threeOpen, tabKey(folder)))).toEqual(missions)
  })

  it('keeps showing the same tab when another one closes', () => {
    expect(activeTab(closeTab(showingMissions, tabKey(operators)))).toEqual(missions)
  })

  it('shows nothing once the last one closes, and ignores unknown keys', () => {
    expect(closeTab(openTab(NO_TABS, folder), tabKey(folder))).toEqual({ tabs: [], activeKey: undefined })
    expect(closeTab(threeOpen, 'nowhere')).toBe(threeOpen)
  })
})

describe('saved tabs', () => {
  it('come back as they were', () => {
    expect(parseOpenTabs(JSON.stringify(threeOpen))).toEqual(threeOpen)
  })

  it('drop what they cannot read, and show the first when the shown one is gone', () => {
    const saved = JSON.stringify({ tabs: [operators, { kind: 'mystery' }, { kind: 'deployment', deploymentId: 'd1', part: 'bogus' }], activeKey: 'gone' })
    expect(parseOpenTabs(saved)).toEqual({ tabs: [operators], activeKey: tabKey(operators) })
  })

  it('start empty when there are none or they are garbled', () => {
    expect(parseOpenTabs(null)).toBe(NO_TABS)
    expect(parseOpenTabs('{oops')).toBe(NO_TABS)
    expect(parseOpenTabs('[]')).toBe(NO_TABS)
  })
})

describe('selectedRowKey', () => {
  it('matches the sidebar row keys', () => {
    expect(selectedRowKey(operators)).toBe('deployment:d1')
    expect(selectedRowKey(folder)).toBe('folder:/repo')
    expect(selectedRowKey({ kind: 'blockers' })).toBe('blockers')
    expect(selectedRowKey({ kind: 'channels' })).toBe('channels')
    expect(selectedRowKey(undefined)).toBeUndefined()
  })
})
