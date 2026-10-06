// A session's live terminal: legion2d's copy of its screen, redrawn as it
// changes, with every keystroke typed straight into the session.

import { Terminal } from '@xterm/xterm'
import { useEffect, useRef } from 'react'

import { askFor, askLegion } from '@/lib/legion'

// legion2d's terminals are this size (TERMINAL_ROWS and TERMINAL_COLUMNS).
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

export function TerminalView({ deploymentId, position }: TerminalViewProps) {
  const holder = useRef<HTMLDivElement>(null)

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
    terminal.focus()
    const typing = terminal.onData(text => {
      void askLegion({ type: 'input', deployment: deploymentId, position, text })
    })
    let lastDrawn = ''
    let isStopped = false
    const redraw = async () => {
      if (isStopped) return
      try {
        const { ansi } = await askFor('screen', { type: 'screen', deployment: deploymentId, position })
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
  return <div ref={holder} className="w-max" />
}
