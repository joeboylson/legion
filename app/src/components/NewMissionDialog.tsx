// Creating a mission in a run. Only the human does this.

import { useState } from 'react'

import { FormDialog } from '@/components/FormDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { askFor } from '@/lib/legion'

export function NewMissionDialog({ runId }: { runId: string }) {
  const [title, setTitle] = useState('')
  const [body, setBody] = useState('')
  const isComplete = title.trim() !== '' && body.trim() !== ''
  return (
    <FormDialog
      trigger={<Button size="sm">New mission</Button>}
      title="New mission"
      submitLabel="Create"
      canSubmit={isComplete}
      onSubmit={async () => {
        await askFor('missions', { type: 'mission_add', run: runId, title: title.trim(), body: body.trim() })
        setTitle('')
        setBody('')
      }}
    >
      <Label htmlFor="mission-title">Title</Label>
      <Input id="mission-title" value={title} onChange={event => setTitle(event.target.value)} placeholder="Add a hello file" autoFocus />
      <Label htmlFor="mission-body">What it should do</Label>
      <Textarea id="mission-body" value={body} onChange={event => setBody(event.target.value)} rows={8} />
    </FormDialog>
  )
}
