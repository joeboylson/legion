// A position's live terminal in a dialog sized to the terminal itself: its
// one column takes the terminal's width (auto, not the usual shrinkable 1fr).

import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { TerminalView } from '@/components/TerminalView'

type TerminalDialogProps = { deploymentId: string; position?: string; onClose: () => void }

export function TerminalDialog({ deploymentId, position, onClose }: TerminalDialogProps) {
  return (
    <Dialog open={position !== undefined} onOpenChange={isOpen => !isOpen && onClose()}>
      <DialogContent
        className="max-h-[calc(100vh-var(--space-6))] w-fit grid-cols-[auto] max-w-[calc(100vw-var(--space-6))] gap-3 overflow-auto p-3 sm:max-w-[calc(100vw-var(--space-6))]"
        // Esc belongs to the session (it stops Claude's turn), not the dialog.
        onEscapeKeyDown={event => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className="label text-label font-normal">terminal · {position}</DialogTitle>
        </DialogHeader>
        {position !== undefined && <TerminalView key={`${deploymentId}:${position}`} deploymentId={deploymentId} position={position} />}
      </DialogContent>
    </Dialog>
  )
}
