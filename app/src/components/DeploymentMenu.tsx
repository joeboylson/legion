// A deployment's "…" menu in the sidebar. Stop ends its sessions and keeps it
// open, so it can start again; close ends it for good, after a check.

import { MoreHorizontal } from 'lucide-react'
import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import type { SessionInfo } from '@/generated/SessionInfo'
import type { Deployment } from '@/generated/Deployment'
import { askLegion } from '@/lib/legion'
import { COMMANDER } from '@/lib/roster'

type DeploymentMenuProps = { deployment: Deployment; sessions: readonly SessionInfo[] }

const stopEverySession = (deployment: Deployment, sessions: readonly SessionInfo[]) =>
  Promise.all(sessions.map(session => askLegion({ type: 'session_stop', deployment: deployment.id, position: session.position })))

// Its commander picks the work back up from the log.
const startCommander = (deployment: Deployment) =>
  askLegion({ type: 'session_start', deployment: deployment.id, operator: COMMANDER, mission: null })

export function DeploymentMenu({ deployment, sessions }: DeploymentMenuProps) {
  const [isConfirmingClose, setIsConfirmingClose] = useState(false)
  const isRunning = sessions.length > 0
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-xs" aria-label={`${deployment.name}'s actions`} title="Actions">
            <MoreHorizontal className="size-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {isRunning ? (
            <DropdownMenuItem onSelect={() => void stopEverySession(deployment, sessions)}>Stop deployment</DropdownMenuItem>
          ) : (
            <DropdownMenuItem onSelect={() => void startCommander(deployment)}>Start deployment</DropdownMenuItem>
          )}
          <DropdownMenuItem variant="destructive" onSelect={() => setIsConfirmingClose(true)}>
            Close deployment…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <Dialog open={isConfirmingClose} onOpenChange={setIsConfirmingClose}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Close {deployment.name}?</DialogTitle>
            <DialogDescription>
              Every session ends and it can't be started again. Its missions, log and branches stay.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setIsConfirmingClose(false)}>
              Keep it
            </Button>
            <Button variant="destructive" onClick={() => void askLegion({ type: 'deployment_close', deployment: deployment.id })}>
              Close deployment
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
