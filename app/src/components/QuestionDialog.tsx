// A question for the human, a decision a session made and carried on with,
// or a strategist's suggestion, in a dialog: the whole text, the mission it's
// about, and room for a proper answer. A decision or suggestion can be
// dismissed instead.

import { useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { DISMISSIBLE_KINDS, type Escalation, KIND_LABELS } from '@/lib/escalations'
import { askFor, askLegion } from '@/lib/legion'

type QuestionDialogProps = { question?: Escalation; onClose: () => void; onDismiss: (question: Escalation) => void }

export function QuestionDialog({ question, onClose, onDismiss }: QuestionDialogProps) {
  return (
    <Dialog open={question !== undefined} onOpenChange={isOpen => !isOpen && onClose()}>
      {question !== undefined && <QuestionContent key={question.key} question={question} onClose={onClose} onDismiss={() => onDismiss(question)} />}
    </Dialog>
  )
}

type QuestionContentProps = { question: Escalation; onClose: () => void; onDismiss: () => void }

function QuestionContent({ question, onClose, onDismiss }: QuestionContentProps) {
  const isDecision = question.kind === 'decision'
  const canAnswer = question.questionId !== undefined && !question.isClosed
  const canDismiss = DISMISSIBLE_KINDS.includes(question.kind)
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
      onClose()
    } catch (error) {
      setProblem(String(error))
    } finally {
      setIsSending(false)
    }
  }

  const askedBy = [question.position, question.deploymentName, question.mission === undefined ? undefined : `mission ${question.mission}`]
    .filter(Boolean)
    .join(' · ')
  const dismissButton = canDismiss && (
    <Button type="button" variant="ghost" onClick={onDismiss}>
      Dismiss
    </Button>
  )

  return (
    <DialogContent className="max-h-[calc(100vh-var(--space-8))] overflow-auto sm:max-w-[var(--measure)]">
      <DialogHeader>
        <DialogTitle className="flex items-baseline gap-3 font-normal">
          <span className="label">{KIND_LABELS[question.kind]}</span>
          <span className="font-mono text-label text-muted-foreground">{askedBy}</span>
        </DialogTitle>
      </DialogHeader>

      <p className="m-0 whitespace-pre-wrap text-lead">{question.text}</p>

      {missionText !== undefined && (
        <section className="flex flex-col gap-2">
          <span className="label">The mission</span>
          <p className="m-0 max-h-[var(--panel)] overflow-auto whitespace-pre-wrap rounded-lg border border-border bg-layer-1 p-4 text-muted-foreground">{missionText}</p>
        </section>
      )}

      {canAnswer ? (
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
            {dismissButton || (
              <Button type="button" variant="ghost" onClick={onClose}>
                Not now
              </Button>
            )}
          </div>
        </form>
      ) : (
        <div className="flex items-center gap-3">
          {question.isClosed && <p className="m-0 text-muted-foreground">Its deployment is closed, so no one is left to read an answer.</p>}
          {dismissButton}
        </div>
      )}
    </DialogContent>
  )
}
