// The operators graph as data: one node per position, a line for each
// pipeline step between them, and a dashed line from the commander to anyone
// the pipeline doesn't already link it to (all work passes through it).

import type { Activity } from '@/generated/Activity'
import type { DecisionDetail } from '@/generated/DecisionDetail'
import type { Entry } from '@/generated/Entry'
import { COMMANDER, operatorOfPosition, type RosterEntry } from '@/lib/roster'

// A pipeline step that ends the mission rather than reaching anyone.
const PIPELINE_END = 'done'

// The ring's spacing: room for one label between neighbours.
const RING_SPACING_PX = 144
const MIN_RING_RADIUS_PX = 144
// How far apart an operator's copies sit within its spot.
const COPY_SPACING_PX = 34

export type GraphNode = { id: string; activity: Activity | 'inactive' }
export type EdgeKind = 'step' | 'spoke'
export type GraphEdge = { id: string; source: string; target: string; kind: EdgeKind }
export type Route = { from: string; to: string }
export type Point = { x: number; y: number }

export const graphNodes = (roster: readonly RosterEntry[]): GraphNode[] =>
  roster.map(entry => ({ id: entry.position, activity: entry.session?.activity ?? 'inactive' }))

// A step names operators; each running copy of one (builder-2) is linked too.
const positionsOf = (operator: string, positions: readonly string[]): string[] =>
  positions.filter(position => operatorOfPosition(position) === operator)

const pairKey = (first: string, second: string): string => [first, second].sort().join('|')

export const graphEdges = (roster: readonly RosterEntry[], steps: readonly DecisionDetail[]): GraphEdge[] => {
  const positions = roster.map(entry => entry.position)
  const stepEdges = steps
    .filter(step => step.next !== PIPELINE_END)
    .flatMap(step =>
      positionsOf(step.operator, positions).flatMap(source =>
        positionsOf(step.next, positions)
          .filter(target => target !== source)
          .map((target): GraphEdge => ({ id: `step:${source}->${target}`, source, target, kind: 'step' })),
      ),
    )
  const uniqueSteps = [...new Map(stepEdges.map(edge => [edge.id, edge])).values()]
  const linkedPairs = new Set(uniqueSteps.map(edge => pairKey(edge.source, edge.target)))
  const spokes = positions.includes(COMMANDER)
    ? positions
        .filter(position => position !== COMMANDER && !linkedPairs.has(pairKey(COMMANDER, position)))
        .map((position): GraphEdge => ({ id: `spoke:${position}`, source: COMMANDER, target: position, kind: 'spoke' }))
    : []
  return [...uniqueSteps, ...spokes]
}

// Who an entry went from and to, when both are on the graph. An operator
// starting is the commander handing it work, so it's drawn as that.
export const routeOf = (entry: Entry, nodeIds: ReadonlySet<string>): Route | undefined => {
  const isOperatorStarting = entry.kind === 'session_started' && entry.from !== COMMANDER
  const route = isOperatorStarting ? { from: COMMANDER, to: entry.from } : { from: entry.from, to: entry.to }
  if (route.to === null || route.from === route.to) return undefined
  if (!nodeIds.has(route.from) || !nodeIds.has(route.to)) return undefined
  return { from: route.from, to: route.to }
}

// Where each position sits, the same way every time: the commander in the
// middle, every operator on a ring in pipeline order, clockwise from the top.
// An operator's copies (builder, builder-2) share its one spot, side by side,
// so together they read as one.
export const ringPositions = (positions: readonly string[], pipelineOrder: readonly string[]): Map<string, Point> => {
  const rank = (operator: string): number => {
    const index = pipelineOrder.indexOf(operator)
    return index === -1 ? pipelineOrder.length : index
  }
  const copiesOf = (operator: string): string[] =>
    positions.filter(position => operatorOfPosition(position) === operator).sort((first, second) => first.localeCompare(second))
  const operators = [...new Set(positions.filter(position => position !== COMMANDER).map(operatorOfPosition))].sort(
    (first, second) => rank(first) - rank(second) || first.localeCompare(second),
  )
  const radius = Math.max(MIN_RING_RADIUS_PX, (operators.length * RING_SPACING_PX) / (2 * Math.PI))
  const placed = operators.flatMap((operator, index): [string, Point][] => {
    const angle = (2 * Math.PI * index) / operators.length - Math.PI / 2
    const spot = { x: radius * Math.cos(angle), y: radius * Math.sin(angle) }
    const copies = copiesOf(operator)
    const firstOffset = -((copies.length - 1) * COPY_SPACING_PX) / 2
    return copies.map((copy, copyIndex) => [copy, { x: Math.round(spot.x + firstOffset + copyIndex * COPY_SPACING_PX), y: Math.round(spot.y) }])
  })
  return new Map([...(positions.includes(COMMANDER) ? [[COMMANDER, { x: 0, y: 0 }] as [string, Point]] : []), ...placed])
}

// Operators running more than one copy (builder and builder-2), each with
// its positions, so the graph can draw a border around them.
export const copyGroups = (positions: readonly string[]): Map<string, string[]> => {
  const byOperator = new Map<string, string[]>()
  positions.forEach(position => {
    const operator = operatorOfPosition(position)
    byOperator.set(operator, [...(byOperator.get(operator) ?? []), position])
  })
  return new Map([...byOperator].filter(([, copies]) => copies.length > 1))
}

// Which copy a position is: builder is 1, builder-2 is 2.
export const copyNumber = (position: string): number => Number(/-(\d+)$/.exec(position)?.[1] ?? 1)
