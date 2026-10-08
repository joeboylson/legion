// The one-click answer to a blocker. A permission question is answered with
// Claude Code's own keys for its dialog: "1" picks "1. Yes" (allow once)
// wherever the cursor is, and Esc denies. Esc ends the operator's turn too,
// so a deny is followed by a Legion message telling it to carry on without
// the call; a halted operator gets that message alone. Once sent, it waits:
// the blocker goes when its operator reports it has moved on, or the buttons
// come back if it hasn't in a while.

import { useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import type { Command } from '@/generated/Command'
import type { Escalation, EscalationKind } from '@/lib/escalations'
import { askFor } from '@/lib/legion'
import { cn } from '@/lib/utils'

// Each button's color: text and border alike.
const TONES = {
  go: 'border-[var(--blue)] text-[var(--blue)]',
  allow: 'border-[var(--green)] text-[var(--green)]',
  deny: 'border-[var(--red)] text-[var(--red)]',
} as const

// What a choice sends: a press in the terminal (text typed, or a named key),
// then a Legion message, either or both.
type Press = { input: string } | { key: string }

type Choice = { label: string; title: string; tone: keyof typeof TONES; press?: Press; message?: (blocker: Escalation) => string }

const CHOICES: Partial<Record<EscalationKind, readonly Choice[]>> = {
  permission: [
    { label: 'Allow', title: 'Allow it this once', tone: 'allow', press: { input: '1' } },
    {
      label: 'Deny',
      title: 'Deny it; the operator is told to do something else',
      tone: 'deny',
      press: { key: 'esc' },
      message: blocker => `The human denied that call (${blocker.text}). Carry on without it.`,
    },
  ],
  halted: [{ label: 'Carry on', title: 'Tell it to carry on where it left off', tone: 'go', message: () => 'Carry on with your mission where you left off.' }],
}

const NOT_CLEARED = "Still blocked; open its terminal"

// Long enough for a session to load its add-on and report in.
const WAIT_MS = 21_000

export function BlockerActions({ blocker }: { blocker: Escalation }) {
  const [problem, setProblem] = useState<string>()
  const [isPressing, setIsPressing] = useState(false)
  // Pressed, and waiting for the blocker to clear; this row goes when it does.
  const [isWaiting, setIsWaiting] = useState(false)

  useEffect(() => {
    if (!isWaiting) return
    const giveUp = window.setTimeout(() => {
      setIsWaiting(false)
      setProblem(NOT_CLEARED)
    }, WAIT_MS)
    return () => window.clearTimeout(giveUp)
  }, [isWaiting])

  const choices = CHOICES[blocker.kind] ?? []
  const { position, deploymentId } = blocker
  if (choices.length === 0 || position === undefined) return null

  const choose = async (choice: Choice) => {
    setIsPressing(true)
    setProblem(undefined)
    try {
      const { press, message } = choice
      if (press !== undefined) {
        const command: Command =
          'input' in press
            ? { type: 'input', deployment: deploymentId, position, text: press.input }
            : { type: 'key', deployment: deploymentId, position, key: press.key }
        await askFor('done', command)
      }
      if (message !== undefined) {
        const entry = { kind: 'message' as const, mission: blocker.mission ?? null, to: position, text: message(blocker), answers: null }
        await askFor('entry', { type: 'post', deployment: deploymentId, entry })
      }
      setIsWaiting(true)
    } catch (error) {
      setProblem(String(error))
    } finally {
      setIsPressing(false)
    }
  }

  if (isWaiting) return <p className="m-0 text-right text-label text-muted-foreground">Waiting…</p>

  return (
    <div className="flex items-center justify-end gap-2">
      {problem !== undefined && <span className="text-label text-danger">{problem}</span>}
      {choices.map(choice => (
        <Button
          key={choice.label}
          variant="outline"
          size="xs"
          className={cn('rounded-full bg-transparent', TONES[choice.tone])}
          title={blocker.isClosed ? 'Its deployment is closed' : choice.title}
          disabled={blocker.isClosed || isPressing}
          onClick={() => void choose(choice)}
        >
          {choice.label}
        </Button>
      ))}
    </div>
  )
}
