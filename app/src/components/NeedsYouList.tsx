// One list of everything waiting on the human, across every run.

import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { askLegion } from '@/lib/legion'
import type { NeedsYouItem, NeedsYouKind } from '@/lib/needs-you'

const KIND_LABELS: Record<NeedsYouKind, string> = {
  permission: 'permission',
  question: 'question',
  blocked: 'blocked',
  limit: 'usage limit',
  suggestion: 'suggestion',
}

type NeedsYouListProps = {
  items: readonly NeedsYouItem[]
  onOpen: (item: NeedsYouItem) => void
  onDismiss: (item: NeedsYouItem) => void
}

function AnswerBox({ item }: { item: NeedsYouItem }) {
  const [answer, setAnswer] = useState('')
  const hasAnswer = answer.trim().length > 0
  const send = () => {
    if (!hasAnswer || item.questionId === undefined) return
    const entry = { kind: 'answer' as const, mission: null, to: null, text: answer.trim(), answers: item.questionId }
    void askLegion({ type: 'post', run: item.runId, entry })
    setAnswer('')
  }
  return (
    <form
      className="flex gap-2"
      onSubmit={event => {
        event.preventDefault()
        send()
      }}
    >
      <Input value={answer} onChange={event => setAnswer(event.target.value)} placeholder="Answer…" />
      <Button type="submit" size="sm" disabled={!hasAnswer}>
        Send
      </Button>
    </form>
  )
}

export function NeedsYouList({ items, onOpen, onDismiss }: NeedsYouListProps) {
  if (items.length === 0) return <p className="px-4 pb-3 text-muted-foreground">Nothing is waiting on you.</p>
  return (
    <ul className="flex flex-col">
      {items.map(item => {
        const who = [item.runName, item.position, item.mission === undefined ? undefined : `m${item.mission}`].filter(Boolean).join(' · ')
        return (
          <li key={item.key} className="flex flex-col gap-2 border-b border-border px-4 py-3">
            <button type="button" className="flex flex-col gap-1 text-left" onClick={() => onOpen(item)}>
              <span className="label">{KIND_LABELS[item.kind]}</span>
              <span className="line-clamp-3">{item.text}</span>
              <span className="font-mono text-label text-muted-foreground">{who}</span>
            </button>
            {item.kind === 'question' && <AnswerBox item={item} />}
            {item.kind === 'suggestion' && (
              <Button variant="ghost" size="xs" className="self-start" onClick={() => onDismiss(item)}>
                Dismiss
              </Button>
            )}
          </li>
        )
      })}
    </ul>
  )
}
