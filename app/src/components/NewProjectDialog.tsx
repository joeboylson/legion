// A new project: a folder on this machine, which legion2d sets up with
// .legion2/ if it has none.

import { Plus } from 'lucide-react'
import { useState } from 'react'

import { FormDialog } from '@/components/FormDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { askFor } from '@/lib/legion'

export function NewProjectDialog({ onAdded }: { onAdded: () => void }) {
  const [path, setPath] = useState('')
  const hasPath = path.trim().length > 0
  return (
    <FormDialog
      trigger={
        <Button variant="ghost" size="icon-xs" aria-label="Add folder" title="Add folder">
          <Plus className="size-4" />
        </Button>
      }
      title="Add folder"
      submitLabel="Add folder"
      canSubmit={hasPath}
      onSubmit={async () => {
        await askFor('folder', { type: 'folder_add', path: path.trim() })
        setPath('')
        onAdded()
      }}
    >
      <Label htmlFor="project-folder">A folder on this machine</Label>
      <Input id="project-folder" value={path} onChange={event => setPath(event.target.value)} placeholder="/Users/you/code/app" autoFocus />
    </FormDialog>
  )
}
