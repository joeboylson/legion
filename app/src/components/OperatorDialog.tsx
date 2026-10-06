// An operator's card on the folder page; clicking it opens its definition
// and settings in a dialog.

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import type { OperatorDetail } from '@/generated/OperatorDetail'
import { paragraphs } from '@/lib/format'

import { Setting, SettingsList } from './SettingsList'

function ToolRules({ name, rules }: { name: string; rules: readonly string[] }) {
  return <Setting name={name} value={rules.length === 0 ? null : rules.join(', ')} />
}

export function OperatorDialog({ operator }: { operator: OperatorDetail }) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <button type="button" className="flex flex-col gap-1 rounded-lg border border-border bg-background p-4 text-left hover:bg-layer-2">
          <span className="font-medium">{operator.name}</span>
          <span className="text-muted-foreground">
            {operator.model ?? 'default model'} · up to {operator.copy_limit} at once
          </span>
          {operator.problem !== null && <span className="text-danger">{operator.problem}</span>}
        </button>
      </DialogTrigger>
      <DialogContent className="max-h-[calc(100vh-var(--space-8))] overflow-auto sm:max-w-[var(--measure)]">
        <DialogHeader>
          <DialogTitle>{operator.name}</DialogTitle>
        </DialogHeader>
        {operator.problem !== null && <p className="text-danger">{operator.problem}</p>}
        <SettingsList>
          <Setting name="Model" value={operator.model ?? 'the default'} />
          <Setting name="At once" value={String(operator.copy_limit)} />
          <Setting name="Permissions" value={operator.permission_mode ?? "the folder's"} />
          <ToolRules name="Allowed" rules={operator.allowed_tools} />
          <ToolRules name="Blocked" rules={operator.disallowed_tools} />
        </SettingsList>
        <div className="flex flex-col gap-3">
          {paragraphs(operator.definition).map(paragraph => (
            <p key={paragraph} className="m-0">
              {paragraph}
            </p>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
