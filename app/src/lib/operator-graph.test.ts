import { describe, expect, it } from 'vitest'

import type { Entry } from '@/generated/Entry'
import type { SessionInfo } from '@/generated/SessionInfo'

import { copyGroups, copyNumber, graphEdges, graphNodes, ringPositions, routeOf } from './operator-graph'
import type { RosterEntry } from './roster'

const running = (position: string): RosterEntry => ({
  position,
  session: { deployment: 'd1', position, mission: 7, activity: 'busy', can_see_state: true, detail: null, is_stuck_starting: false, model: null, context_percent: null, part: null } satisfies SessionInfo,
})
const inactive = (position: string): RosterEntry => ({ position })

const step = (operator: string, next: string) => ({ operator, condition: 'ready', next })

const entry = (from: string, to: string | null): Entry => ({
  id: 1,
  deployment: 'd1',
  at_ms: 0,
  mission: 7,
  from,
  to,
  kind: 'message',
  text: '',
  answers: null,
})

describe('graphNodes', () => {
  it('marks positions with no session inactive', () => {
    expect(graphNodes([running('builder'), inactive('tester')])).toEqual([
      { id: 'builder', activity: 'busy' },
      { id: 'tester', activity: 'inactive' },
    ])
  })
})

describe('graphEdges', () => {
  const roster = [running('commander'), running('planner'), running('builder'), inactive('tester')]

  it('draws each pipeline step once, and no step to done', () => {
    const edges = graphEdges(roster, [step('planner', 'builder'), step('planner', 'builder'), step('builder', 'done')])
    expect(edges.filter(edge => edge.kind === 'step').map(edge => edge.id)).toEqual(['step:planner->builder'])
  })

  it('links the commander to everyone a step does not already link it to', () => {
    const edges = graphEdges(roster, [step('builder', 'commander')])
    expect(edges.filter(edge => edge.kind === 'spoke').map(edge => edge.target)).toEqual(['planner', 'tester'])
  })

  it('links every running copy of an operator', () => {
    const edges = graphEdges([running('planner'), running('builder'), running('builder-2')], [step('planner', 'builder')])
    expect(edges.map(edge => edge.target)).toEqual(['builder', 'builder-2'])
  })
})

describe('routeOf', () => {
  const nodeIds = new Set(['commander', 'builder'])

  it('routes an entry between two positions on the graph', () => {
    expect(routeOf(entry('builder', 'commander'), nodeIds)).toEqual({ from: 'builder', to: 'commander' })
  })

  it('draws an operator starting as the commander handing it work', () => {
    expect(routeOf({ ...entry('builder', null), kind: 'session_started' }, nodeIds)).toEqual({ from: 'commander', to: 'builder' })
    expect(routeOf({ ...entry('commander', null), kind: 'session_started' }, nodeIds)).toBeUndefined()
  })

  it('skips entries with no one, or someone off the graph, at either end', () => {
    expect(routeOf(entry('builder', null), nodeIds)).toBeUndefined()
    expect(routeOf(entry('human', 'commander'), nodeIds)).toBeUndefined()
  })
})

describe('ringPositions', () => {
  it('puts the commander in the middle and the first operator at the top', () => {
    const placed = ringPositions(['tester', 'commander', 'planner', 'builder'], ['planner', 'builder', 'tester'])
    expect(placed.get('commander')).toEqual({ x: 0, y: 0 })
    expect(placed.get('planner')?.x).toBe(0)
    expect(placed.get('planner')?.y).toBeLessThan(0)
  })

  it('goes clockwise in pipeline order', () => {
    const placed = ringPositions(['tester', 'builder', 'planner'], ['planner', 'builder', 'tester'])
    expect([...placed.keys()]).toEqual(['planner', 'builder', 'tester'])
    expect(placed.get('builder')?.x).toBeGreaterThan(0)
  })

  it('puts copies side by side in the spot of their operator', () => {
    const placed = ringPositions(['builder-2', 'tester', 'builder', 'planner'], ['planner', 'builder', 'tester'])
    const builder = placed.get('builder')
    const copy = placed.get('builder-2')
    expect(copy?.y).toBe(builder?.y)
    expect((copy?.x ?? 0) - (builder?.x ?? 0)).toBe(34)
    expect(placed.get('planner')).toEqual({ x: 0, y: -144 })
  })
})

describe('copyGroups', () => {
  it('groups an operator with its copies, and leaves single ones out', () => {
    const groups = copyGroups(['commander', 'builder', 'builder-2', 'tester', 'docs-writer'])
    expect([...groups]).toEqual([['builder', ['builder', 'builder-2']]])
  })
})

describe('copyNumber', () => {
  it('numbers an operator 1 and each copy by its suffix', () => {
    expect(copyNumber('builder')).toBe(1)
    expect(copyNumber('builder-2')).toBe(2)
    expect(copyNumber('docs-writer')).toBe(1)
  })
})
