// A question for the human, or a decision a session made and carried on
// with, on a page of its own: the whole text, the mission it's about, and
// room for a proper answer. A decision can be dismissed instead.

import { useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { askFor, askLegion } from '@/lib/legion'
import type { Escalation } from '@/lib/escalations'

type QuestionPageProps = {
  question: Escalation
  onAnswered: () => void
  onBack: () => void
  onDismiss: () => void
}

export function QuestionPage({ question, onAnswered, onBack, onDismiss }: QuestionPageProps) {
  const isDecision = question.kind === 'decision'
  const [answer, setAnswer] = useState('')
  const [missionText, setMissionText] = useState<string>()
  const [problem, setProblem] = useState<string>()
  const [isSending, setIsSending] = useState(false)
  const hasAnswer = answer.trim().length > 0

  useEffect(() => {
    if (question.mission === undefined) return
    askFor('mission', { type: 'mission_read', deployment: question.deploymentId, mission: question.mission })
      .then(reply => setMissionText(reply.body))
      .catch((error: unknown) => setMissionText(`Couldn't read the mission: ${String(error)}`))
  }, [question.deploymentId, question.mission])

  const send = async () => {
    if (!hasAnswer || question.questionId === undefined) return
    setIsSending(true)
    setProblem(undefined)
    try {
      const entry = { kind: 'answer' as const, mission: null, to: null, text: answer.trim(), answers: question.questionId }
      await askLegion({ type: 'post', deployment: question.deploymentId, entry })
      onAnswered()
    } catch (error) {
      setProblem(String(error))
    } finally {
      setIsSending(false)
    }
  }

  const askedBy = [question.position, question.deploymentName, question.mission === undefined ? undefined : `mission ${question.mission}`]
    .filter(Boolean)
    .join(' · ')

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <header className="flex items-center gap-4 border-b border-border px-4 py-3">
        <Button variant="ghost" size="sm" onClick={onBack}>
          ← Back
        </Button>
        <span className="label">{isDecision ? 'Decision' : 'Question'}</span>
        <span className="font-mono text-label text-muted-foreground">{askedBy}</span>
      </header>

      <div className="flex max-w-[var(--measure)] flex-col gap-5 p-5">
        <p className="whitespace-pre-wrap text-lead">{question.text}</p>

        {missionText !== undefined && (
          <section className="flex flex-col gap-2">
            <span className="label">The mission</span>
            <p className="whitespace-pre-wrap rounded-lg border border-border bg-layer-1 p-4 text-muted-foreground">{missionText}</p>
          </section>
        )}

        <form
          className="flex flex-col gap-3"
          onSubmit={event => {
            event.preventDefault()
            void send()
          }}
        >
          <Label htmlFor="answer">{isDecision ? 'Your answer, if you want it changed' : 'Your answer'}</Label>
          <Textarea id="answer" className="min-h-9" value={answer} onChange={event => setAnswer(event.target.value)} autoFocus />
          {problem !== undefined && <p className="text-danger">{problem}</p>}
          <div className="flex gap-3">
            <Button type="submit" disabled={!hasAnswer || isSending}>
              Send answer
            </Button>
            {isDecision ? (
              <Button type="button" variant="ghost" onClick={onDismiss}>
                Dismiss
              </Button>
            ) : (
              <Button type="button" variant="ghost" onClick={onBack}>
                Not now
              </Button>
            )}
          </div>
        </form>
      </div>
    </div>
  )
}
