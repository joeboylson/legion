// A position's live terminal in a dialog sized to the terminal itself: its
// one column takes the terminal's width (auto, not the usual shrinkable 1fr).

import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { TerminalView } from '@/components/TerminalView'

type TerminalDialogProps = { deploymentId: string; position?: string; onClose: () => void }

export function TerminalDialog({ deploymentId, position, onClose }: TerminalDialogProps) {
  return (
    <Dialog open={position !== undefined} onOpenChange={isOpen => !isOpen && onClose()}>
      <DialogContent
        // Esc closes it, like every other dialog; the terminal never sends it on.
        className="max-h-[calc(100vh-var(--space-6))] w-fit grid-cols-[auto] max-w-[calc(100vw-var(--space-6))] gap-3 overflow-hidden p-3 sm:max-w-[calc(100vw-var(--space-6))]"
      >
        <DialogHeader>
          <DialogTitle className="label text-label font-normal">terminal · {position}</DialogTitle>
        </DialogHeader>
        {position !== undefined && <TerminalView key={`${deploymentId}:${position}`} deploymentId={deploymentId} position={position} />}
      </DialogContent>
    </Dialog>
  )
}
