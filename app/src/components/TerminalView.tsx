// A session's live terminal: legiond's copy of its screen, redrawn as it
// changes, with every keystroke typed straight into the session except Esc,
// which would stop Claude's turn: it closes the dialog instead. What scrolled
// off the top sits above it, and the wheel scrolls through both.

import { Terminal } from '@xterm/xterm'
import { useEffect, useRef } from 'react'

import { askFor, askLegion } from '@/lib/legion'

// legiond's terminals are this size (TERMINAL_ROWS and TERMINAL_COLUMNS).
const TERMINAL_ROWS = 40
const TERMINAL_COLUMNS = 120
const REDRAW_INTERVAL_MS = 233
const PIXELS_PER_REM = 16

const cssVariable = (name: string): string => getComputedStyle(document.documentElement).getPropertyValue(name).trim()

const terminalTheme = () => ({
  background: cssVariable('--surface'),
  foreground: cssVariable('--fg'),
  cursor: cssVariable('--accent'),
  selectionBackground: cssVariable('--selection'),
})

type TerminalViewProps = { deploymentId: string; position: string }

// Within this many pixels of the bottom counts as at the bottom, so new
// lines keep it there.
const BOTTOM_SLACK_PX = 8

export function TerminalView({ deploymentId, position }: TerminalViewProps) {
  const holder = useRef<HTMLDivElement>(null)
  const scroller = useRef<HTMLDivElement>(null)
  const earlierLines = useRef<HTMLPreElement>(null)
  const isAtBottom = useRef(true)

  useEffect(() => {
    if (holder.current === null) return
    const terminal = new Terminal({
      rows: TERMINAL_ROWS,
      cols: TERMINAL_COLUMNS,
      fontFamily: cssVariable('--font-mono'),
      // xterm takes pixels; the token is in rem.
      fontSize: Number.parseFloat(cssVariable('--text-body')) * PIXELS_PER_REM,
      theme: terminalTheme(),
    })
    terminal.open(holder.current)
    terminal.attachCustomKeyEventHandler(event => event.key !== 'Escape')
    // The page scrolls, not the terminal: it only ever holds the screen.
    terminal.attachCustomWheelEventHandler(() => false)
    terminal.focus()
    const typing = terminal.onData(text => {
      void askLegion({ type: 'input', deployment: deploymentId, position, text })
    })
    let lastDrawn = ''
    let isStopped = false
    const redraw = async () => {
      if (isStopped) return
      try {
        const { ansi, history: earlier } = await askFor('screen', { type: 'screen', deployment: deploymentId, position })
        // Closed while it was asking: nothing left to draw on.
        if (isStopped) return
        // Drawn straight in, like the terminal. New lines keep the view at
        // the bottom, unless you've scrolled up.
        const shown = earlierLines.current
        if (shown !== null && shown.textContent !== (earlier ?? '')) {
          shown.textContent = earlier ?? ''
          if (isAtBottom.current && scroller.current !== null) scroller.current.scrollTop = scroller.current.scrollHeight
        }
        if (ansi !== lastDrawn) {
          lastDrawn = ansi
          terminal.write(ansi)
        }
      } catch (error) {
        terminal.write(`\r\n${String(error)}\r\n`)
        isStopped = true
      }
    }
    const timer = window.setInterval(() => void redraw(), REDRAW_INTERVAL_MS)
    void redraw()
    return () => {
      isStopped = true
      window.clearInterval(timer)
      typing.dispose()
      terminal.dispose()
    }
  }, [deploymentId, position])

  // As wide as the terminal it holds, so the dialog around it fits it exactly.
  return (
    <div
      ref={scroller}
      className="max-h-[calc(100vh-var(--space-8))] w-max overflow-auto"
      onScroll={event => {
        const view = event.currentTarget
        isAtBottom.current = view.scrollHeight - view.scrollTop - view.clientHeight <= BOTTOM_SLACK_PX
      }}
    >
      <pre ref={earlierLines} className="m-0 font-mono text-body whitespace-pre text-muted-foreground empty:hidden" />
      <div ref={holder} />
    </div>
  )
}
