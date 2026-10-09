// What you can do with one thing, as a right-click menu on its row or card
// and as its "…" button: the same list both ways, each opening its action
// with that thing already chosen.

import { MoreHorizontal, Plus } from 'lucide-react'
import type { ReactNode } from 'react'

import { Button } from '@/components/ui/button'
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from '@/components/ui/context-menu'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { useActions } from '@/hooks/useActions'
import { actionById, actionsFor } from '@/lib/actions/catalog'
import type { Prefill, Subject } from '@/lib/actions/types'

// A way to look at the thing, listed above its actions.
export type MenuOpen = { label: string; onOpen: () => void }

type MenuProps = { subject: Subject; prefill: Prefill; open?: MenuOpen; label: string }

export function ActionContextMenu({ subject, prefill, open, children }: Omit<MenuProps, 'label'> & { children: ReactNode }) {
  const { runAction } = useActions()
  const actions = actionsFor(subject)
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent>
        {open !== undefined && (
          <>
            <ContextMenuItem onSelect={open.onOpen}>{open.label}</ContextMenuItem>
            <ContextMenuSeparator />
          </>
        )}
        {actions.map(action => (
          <ContextMenuItem key={action.id} variant={action.isDestructive === true ? 'destructive' : 'default'} onSelect={() => runAction(action.id, prefill)}>
            {action.label}…
          </ContextMenuItem>
        ))}
      </ContextMenuContent>
    </ContextMenu>
  )
}

export function ActionMenuButton({ subject, prefill, open, label }: MenuProps) {
  const { runAction } = useActions()
  const actions = actionsFor(subject)
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-xs" aria-label={`${label}: actions`} title="Actions">
          <MoreHorizontal className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {open !== undefined && (
          <>
            <DropdownMenuItem onSelect={open.onOpen}>{open.label}</DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        {actions.map(action => (
          <DropdownMenuItem key={action.id} variant={action.isDestructive === true ? 'destructive' : 'default'} onSelect={() => runAction(action.id, prefill)}>
            {action.label}…
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

// The "+" on a section: adds one of what it lists.
export function AddButton({ actionId, prefill }: { actionId: string; prefill: Prefill }) {
  const { runAction } = useActions()
  const label = actionById(actionId)?.label ?? 'Add'
  return (
    <Button variant="ghost" size="icon-xs" aria-label={label} title={label} onClick={() => runAction(actionId, prefill)}>
      <Plus className="size-4" />
    </Button>
  )
}

// A page's own button for its most common action.
export function ActionButton({ actionId, prefill, variant = 'outline' }: { actionId: string; prefill: Prefill; variant?: 'outline' | 'default' | 'ghost' }) {
  const { runAction } = useActions()
  const label = actionById(actionId)?.label ?? actionId
  return (
    <Button type="button" variant={variant} size="sm" onClick={() => runAction(actionId, prefill)}>
      {label}
    </Button>
  )
}
