import { describe, expect, it } from 'vitest'

import type { SessionInfo } from '@/generated/SessionInfo'

import { byOperatorOrder, groupedRoster, operatorOfPosition, rosterOf } from './roster'

const session = (position: string): SessionInfo => ({
  deployment: 'd1',
  position,
  mission: null,
  activity: 'busy',
  can_see_state: true,
  detail: null,
  is_stuck_starting: false,
  model: null,
  context_percent: null,
})

describe('operatorOfPosition', () => {
  it('drops a copy number', () => {
    expect(operatorOfPosition('builder-2')).toBe('builder')
  })

  it('keeps a name that only has a dash', () => {
    expect(operatorOfPosition('docs-writer')).toBe('docs-writer')
  })
})

describe('byOperatorOrder', () => {
  it('puts the commander first and a copy after its operator', () => {
    expect(['tester', 'builder-2', 'commander', 'builder'].sort(byOperatorOrder)).toEqual(['commander', 'builder', 'builder-2', 'tester'])
  })
})

describe('rosterOf', () => {
  it('lists the commander, inactive, when it is not running', () => {
    expect(rosterOf([], ['planner'])).toEqual([{ position: 'commander' }, { position: 'planner' }])
  })

  it('lists the commander first, then everyone A to Z, running or not', () => {
    const roster = rosterOf([session('tester'), session('commander'), session('builder')], ['planner', 'builder', 'tester', 'architect'])
    expect(roster.map(entry => entry.position)).toEqual(['commander', 'architect', 'builder', 'planner', 'tester'])
    expect(roster.filter(entry => entry.session === undefined).map(entry => entry.position)).toEqual(['architect', 'planner'])
  })

  it('counts a copy as its operator running', () => {
    const roster = rosterOf([session('commander'), session('builder-2')], ['builder'])
    expect(roster.map(entry => entry.position)).toEqual(['commander', 'builder-2'])
  })
})

describe('groupedRoster', () => {
  it('keeps an operator with its copies, in order', () => {
    const groups = groupedRoster([{ position: 'commander' }, { position: 'builder' }, { position: 'builder-2' }, { position: 'tester' }])
    expect(groups.map(group => [group.operator, group.entries.map(entry => entry.position)])).toEqual([
      ['commander', ['commander']],
      ['builder', ['builder', 'builder-2']],
      ['tester', ['tester']],
    ])
  })
})
