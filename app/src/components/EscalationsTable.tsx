// Open questions, decisions or blockers, as a table like the missions.
// Click one to open it.

import type { ReactNode } from 'react'

import { type Escalation, KIND_LABELS } from '@/lib/escalations'

type EscalationsTableProps = {
  items: readonly Escalation[]
  // Questions mix kinds (a permission, a blocked mission …); decisions don't.
  showsKind: boolean
  // On a page with every deployment's items.
  showsDeployment?: boolean
  emptyText: string
  // Buttons at the right end of each row, such as a blocker's quick answer.
  actions?: (item: Escalation) => ReactNode
  onOpen: (item: Escalation) => void
}

export function EscalationsTable({ items, showsKind, showsDeployment = false, emptyText, actions, onOpen }: EscalationsTableProps) {
  if (items.length === 0) return <p className="text-muted-foreground">{emptyText}</p>
  return (
    <table>
      <thead>
        <tr>
          {showsKind && <th>Kind</th>}
          <th>What</th>
          {showsDeployment && <th>Deployment</th>}
          <th>From</th>
          <th>Mission</th>
          {actions !== undefined && <th aria-label="Actions" />}
        </tr>
      </thead>
      <tbody>
        {items.map(item => (
          <tr key={item.key}>
            {showsKind && <td className="font-mono text-label text-muted-foreground">{KIND_LABELS[item.kind]}</td>}
            <td>
              <button type="button" className="line-clamp-2 text-left underline-offset-4 hover:underline" onClick={() => onOpen(item)}>
                {item.text}
              </button>
            </td>
            {showsDeployment && <td className={item.isClosed ? 'text-muted-foreground' : undefined}>{item.deploymentName}</td>}
            <td className="font-mono">{item.position ?? ''}</td>
            <td className="font-mono">{item.mission ?? ''}</td>
            {actions !== undefined && <td>{actions(item)}</td>}
          </tr>
        ))}
      </tbody>
    </table>
  )
}
