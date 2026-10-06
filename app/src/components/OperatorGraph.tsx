// The deployment's operators as a live graph. Each node is a position,
// colored by what it's doing, always in the same place: the commander in the
// middle, the rest on a ring in pipeline order. Straight lines are the
// pipeline's steps. Every message, handoff or question between two positions
// lights its line and sends a dot along it, from sender to receiver. Click a
// running position for its terminal.

import cytoscape, { type Core, type ElementDefinition, type NodeSingular, type StylesheetJson } from 'cytoscape'
import { useEffect, useRef } from 'react'

import { ACTIVITY_COLORS } from '@/lib/format'
import { onLegionEvent } from '@/lib/legion'
import { copyGroups, copyNumber, type GraphEdge, type GraphNode, graphEdges, graphNodes, ringPositions, routeOf } from '@/lib/operator-graph'
import { COMMANDER, type RosterEntry } from '@/lib/roster'

// Fibonacci sizes and times, as everywhere in the house style.
const NODE_SIZE_PX = 21
const COMMANDER_SIZE_PX = 34
const PACKET_SIZE_PX = 8
const PACKET_TRAVEL_MS = 610
const LINE_LIT_MS = 987
const LABEL_SIZE_PX = 8
const GROUP_PADDING_PX = 13

// A working position pulses: a ring that grows out from it and fades.
const PULSE_EVERY_MS = 1597
const PULSE_GROW_MS = 987
const PULSE_REACH_PX = 13
const PULSE_START_OPACITY = 0.5

// Wheel zooming eases toward where the wheel is heading instead of jumping.
const ZOOM_EASE_MS = 144
const ZOOM_PER_WHEEL_PIXEL = 0.002
const MIN_ZOOM = 0.3
const MAX_ZOOM = 3
// Fitting never blows a small squad up past its true size.
const MAX_FIT_ZOOM = 1
const FIT_PADDING_PX = 34

// cytoscape draws on a canvas, so it needs real colors, not CSS variables.
// An unknown variable throws: cytoscape would quietly draw nothing.
const cssColor = (value: string): string => {
  const variable = /^var\((--[\w-]+)\)$/.exec(value)?.[1]
  if (variable === undefined) return value
  const color = getComputedStyle(document.documentElement).getPropertyValue(variable).trim()
  if (color === '') throw new Error(`no CSS variable ${variable} to color the graph with`)
  return color
}

const nodeColor = (node: GraphNode): string => cssColor(node.activity === 'inactive' ? 'var(--fg-muted)' : ACTIVITY_COLORS[node.activity])

const uniqueById = (elements: ElementDefinition[]): ElementDefinition[] => [...new Map(elements.map(element => [element.data.id, element])).values()]

// The box around an operator's copies has its own id, apart from positions.
const GROUP_PREFIX = 'group:'
const groupId = (operator: string): string => `${GROUP_PREFIX}${operator}`

const toElements = (nodes: readonly GraphNode[], edges: readonly GraphEdge[]): ElementDefinition[] => {
  const groups = copyGroups(nodes.map(node => node.id))
  const groupOf = new Map([...groups].flatMap(([operator, copies]) => copies.map(copy => [copy, groupId(operator)] as const)))
  return [
    ...[...groups.keys()].map(operator => ({ data: { id: groupId(operator), label: operator }, classes: 'group' })),
    ...nodes.map(node => ({
      data: {
        id: node.id,
        parent: groupOf.get(node.id),
        // Inside its operator's border, a copy is just its number.
        label: node.id === COMMANDER ? `♛ ${node.id}` : groupOf.has(node.id) ? String(copyNumber(node.id)) : node.id,
        color: nodeColor(node),
      },
      classes: [
        node.activity === 'inactive' ? 'inactive' : '',
        node.activity === 'busy' ? 'working' : '',
        node.id === COMMANDER ? 'commander' : '',
      ].join(' '),
    })),
    // Lines meet an operator's border, not each copy, so the copies read as one.
    ...uniqueById(
      edges.map(edge => {
        const source = groupOf.get(edge.source) ?? edge.source
        const target = groupOf.get(edge.target) ?? edge.target
        return { data: { id: `${edge.kind}:${source}->${target}`, source, target }, classes: edge.kind }
      }),
    ),
  ]
}

const graphStyle = (): StylesheetJson => [
  {
    selector: 'node',
    style: {
      width: NODE_SIZE_PX,
      height: NODE_SIZE_PX,
      'background-color': 'data(color)',
      label: 'data(label)',
      color: cssColor('var(--fg)'),
      'font-family': cssColor('var(--font-mono)'),
      'font-size': LABEL_SIZE_PX,
      'text-valign': 'bottom',
      'text-margin-y': 3,
    },
  },
  {
    selector: 'node.group',
    style: {
      shape: 'round-rectangle',
      'background-opacity': 0,
      'border-width': 1,
      'border-style': 'dashed',
      'border-color': cssColor('var(--fg-muted)'),
      padding: `${GROUP_PADDING_PX}px`,
      label: 'data(label)',
      color: cssColor('var(--fg-muted)'),
      'text-valign': 'top',
      'text-margin-y': -3,
    },
  },
  {
    selector: 'node.working',
    style: {
      'underlay-color': 'data(color)',
      'underlay-shape': 'ellipse',
      'underlay-padding': PULSE_REACH_PX / 2,
      'underlay-opacity': 0,
    },
  },
  {
    selector: 'node.commander',
    style: { width: COMMANDER_SIZE_PX, height: COMMANDER_SIZE_PX },
  },
  {
    selector: 'node.inactive',
    style: {
      'background-opacity': 0,
      'border-width': 1,
      'border-color': 'data(color)',
      color: cssColor('var(--fg-muted)'),
    },
  },
  {
    selector: 'edge',
    style: {
      width: 1,
      'line-color': cssColor('var(--border)'),
      'curve-style': 'straight',
      'target-arrow-shape': 'triangle',
      'target-arrow-color': cssColor('var(--border)'),
      'arrow-scale': 0.6,
    },
  },
  {
    selector: 'edge.spoke',
    style: { 'line-style': 'dashed', 'target-arrow-shape': 'none' },
  },
  {
    selector: 'edge.lit',
    style: {
      width: 3,
      'line-color': cssColor('var(--accent)'),
      'target-arrow-color': cssColor('var(--accent)'),
    },
  },
  {
    selector: 'node.packet',
    style: {
      width: PACKET_SIZE_PX,
      height: PACKET_SIZE_PX,
      label: '',
      'background-color': cssColor('var(--accent)'),
    },
  },
]

const clampZoom = (level: number): number => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, level))

// Zooms toward the pointer, easing from wherever the last ease left off, so
// a fast spin of the wheel builds into one smooth glide.
const zoomSmoothly = (cy: Core, event: WheelEvent, target: { level: number }) => {
  event.preventDefault()
  const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect()
  target.level = clampZoom(target.level * Math.exp(-event.deltaY * ZOOM_PER_WHEEL_PIXEL))
  cy.stop()
  cy.animate(
    {
      zoom: {
        level: target.level,
        renderedPosition: {
          x: event.clientX - bounds.left,
          y: event.clientY - bounds.top,
        },
      },
    },
    { duration: ZOOM_EASE_MS, easing: 'ease-out' },
  )
}

// One beat of every working position's pulse.
const pulseWorking = (cy: Core) => {
  cy.nodes('.working').forEach(node => {
    node.stop()
    node.style({ 'underlay-padding': 0, 'underlay-opacity': PULSE_START_OPACITY })
    node.animate({ style: { 'underlay-padding': PULSE_REACH_PX, 'underlay-opacity': 0 } }, { duration: PULSE_GROW_MS, easing: 'ease-out' })
  })
}

// Lights the line between two positions and sends a dot from one to the other.
const showMessage = (cy: Core, from: string, to: string) => {
  const sender = cy.getElementById(from)
  const receiver = cy.getElementById(to)
  // A copy's lines meet its operator's border.
  const endOf = (node: NodeSingular) => (node.isChild() ? node.parent().first() : node)
  const line = endOf(sender).edgesWith(endOf(receiver))
  line.addClass('lit')
  window.setTimeout(() => line.removeClass('lit'), LINE_LIT_MS)
  const packet = cy.add({
    group: 'nodes',
    data: { id: `packet:${crypto.randomUUID()}` },
    classes: 'packet',
    position: { ...sender.position() },
  })
  packet.animate({ position: { ...receiver.position() } }, { duration: PACKET_TRAVEL_MS, complete: () => packet.remove() })
}

type OperatorGraphProps = {
  deploymentId: string
  roster: readonly RosterEntry[]
  steps: Parameters<typeof graphEdges>[1]
  // The pipeline's operators in the order work flows through them.
  pipelineOrder: readonly string[]
  onOpen: (position: string) => void
}

export function OperatorGraph({ deploymentId, roster, steps, pipelineOrder, onOpen }: OperatorGraphProps) {
  const holder = useRef<HTMLDivElement>(null)
  const graph = useRef<Core>(undefined)
  // The tap handler is bound once; it reads the latest onOpen through this.
  const openRunning = useRef(onOpen)
  useEffect(() => {
    openRunning.current = onOpen
  })

  const nodes = graphNodes(roster)
  const edges = graphEdges(roster, steps)
  // Lay out again only when who's on the graph changes, not what they're doing.
  const shapeKey = [...nodes.map(node => node.id), ...edges.map(edge => edge.id)].join(',')

  useEffect(() => {
    if (holder.current === null) return
    const container = holder.current
    const cy = cytoscape({
      container,
      style: graphStyle(),
      minZoom: MIN_ZOOM,
      maxZoom: MAX_ZOOM,
      // Wheel zooming is ours, eased; cytoscape's own steps in jumps.
      userZoomingEnabled: false,
      boxSelectionEnabled: false,
    })
    graph.current = cy
    const zoomTarget = { level: cy.zoom() }
    cy.on('layoutstop zoom', () => {
      if (!cy.animated()) zoomTarget.level = cy.zoom()
    })
    const onWheel = (event: WheelEvent) => zoomSmoothly(cy, event, zoomTarget)
    container.addEventListener('wheel', onWheel, { passive: false })
    // The graph fills the tab, so it redraws whenever the window resizes.
    const resizing = new ResizeObserver(() => cy.resize())
    resizing.observe(container)
    // Pulses even with reduced motion on: it's how the graph shows who's working.
    const pulsing = window.setInterval(() => pulseWorking(cy), PULSE_EVERY_MS)
    cy.on('tap', 'node', event => {
      const position: string = event.target.id()
      if (!position.startsWith('packet:') && !position.startsWith(GROUP_PREFIX)) openRunning.current(position)
    })
    return () => {
      window.clearInterval(pulsing)
      resizing.disconnect()
      container.removeEventListener('wheel', onWheel)
      graph.current = undefined
      cy.destroy()
    }
  }, [])

  useEffect(() => {
    const cy = graph.current
    if (cy === undefined) return
    cy.elements().remove()
    cy.add(toElements(nodes, edges))
    const placed = ringPositions(
      nodes.map(node => node.id),
      pipelineOrder,
    )
    cy.layout({ name: 'preset', positions: Object.fromEntries(placed), fit: false }).run()
    cy.fit(undefined, FIT_PADDING_PX)
    if (cy.zoom() > MAX_FIT_ZOOM) {
      cy.zoom(MAX_FIT_ZOOM)
      cy.center()
    }
    // Rebuilt from shapeKey alone: nodes and edges are fresh arrays each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shapeKey])

  // What each position is doing changes often; recolor without moving anything.
  useEffect(() => {
    const cy = graph.current
    if (cy === undefined) return
    nodes.forEach(node => {
      cy.getElementById(node.id)
        .data('color', nodeColor(node))
        .toggleClass('inactive', node.activity === 'inactive')
        .toggleClass('working', node.activity === 'busy')
    })
  })

  useEffect(() => {
    const stop = onLegionEvent(event => {
      const cy = graph.current
      if (event.type !== 'entry' || event.entry.deployment !== deploymentId || cy === undefined) return
      const positions = new Set(
        cy
          .nodes()
          .not('.packet, .group')
          .map(node => node.id()),
      )
      const route = routeOf(event.entry, positions)
      if (route !== undefined) showMessage(cy, route.from, route.to)
    })
    return () => void stop.then(unlisten => unlisten())
  }, [deploymentId])

  return (
    // The list view says the same in text; this is the picture of it.
    <div ref={holder} className="min-h-[377px] w-full flex-1 rounded-lg border border-border bg-background" role="img" aria-label="Operators graph" />
  )
}
