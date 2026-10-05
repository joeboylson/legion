// A small dialog with a form: the shape every "add" or "start" takes here.

import { type ReactNode, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'

type FormDialogProps = {
  trigger: ReactNode
  title: string
  submitLabel: string
  canSubmit: boolean
  // Resolves when done; a thrown error shows in the dialog.
  onSubmit: () => Promise<void>
  children: ReactNode
}

export function FormDialog({ trigger, title, submitLabel, canSubmit, onSubmit, children }: FormDialogProps) {
  const [isOpen, setIsOpen] = useState(false)
  const [problem, setProblem] = useState<string>()
  const [isSubmitting, setIsSubmitting] = useState(false)

  const submit = async () => {
    setIsSubmitting(true)
    setProblem(undefined)
    try {
      await onSubmit()
      setIsOpen(false)
    } catch (error) {
      setProblem(String(error))
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form
          className="flex flex-col gap-4"
          onSubmit={event => {
            event.preventDefault()
            void submit()
          }}
        >
          {children}
          {problem !== undefined && <p className="text-danger">{problem}</p>}
          <DialogFooter>
            <Button type="submit" disabled={!canSubmit || isSubmitting}>
              {submitLabel}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
