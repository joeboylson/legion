// Adding a folder: legion2d sets up .legion2/ in it if it has none.

import { useState } from 'react'

import { FormDialog } from '@/components/FormDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { askFor } from '@/lib/legion'

export function AddFolderDialog({ onAdded }: { onAdded: () => void }) {
  const [path, setPath] = useState('')
  const hasPath = path.trim().length > 0
  return (
    <FormDialog
      trigger={<Button variant="ghost" size="xs">Add folder</Button>}
      title="Add a folder"
      submitLabel="Add"
      canSubmit={hasPath}
      onSubmit={async () => {
        await askFor('folder', { type: 'folder_add', path: path.trim() })
        setPath('')
        onAdded()
      }}
    >
      <Label htmlFor="folder-path">A folder on this machine</Label>
      <Input id="folder-path" value={path} onChange={event => setPath(event.target.value)} placeholder="/Users/you/code/app" autoFocus />
    </FormDialog>
  )
}
