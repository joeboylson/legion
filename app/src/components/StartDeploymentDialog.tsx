// Starting a deployment in a folder: one of its pipelines, and an optional
// name. Opened from the "+" on the folder's Deployments row.

import { Plus } from 'lucide-react'
import { useState } from 'react'

import { FormDialog } from '@/components/FormDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { Deployment } from '@/generated/Deployment'
import type { Folder } from '@/generated/Folder'
import { askFor } from '@/lib/legion'

type StartDeploymentDialogProps = { folder: Folder; onStarted: (deployment: Deployment) => void }

export function StartDeploymentDialog({ folder, onStarted }: StartDeploymentDialogProps) {
  const [pipeline, setPipeline] = useState('')
  const [name, setName] = useState('')
  const hasPipeline = pipeline !== ''
  return (
    <FormDialog
      trigger={
        <Button variant="ghost" size="icon-xs" aria-label={`Start a deployment in ${folder.name}`} title="Start a deployment">
          <Plus className="size-4" />
        </Button>
      }
      title={`Start a deployment in ${folder.name}`}
      submitLabel="Deploy"
      canSubmit={hasPipeline}
      onSubmit={async () => {
        const trimmedName = name.trim()
        const { deployment } = await askFor('deployment', {
          type: 'deployment_start',
          folder: folder.path,
          pipeline,
          name: trimmedName === '' ? null : trimmedName,
        })
        onStarted(deployment)
      }}
    >
      <Label>Pipeline</Label>
      <Select value={pipeline} onValueChange={setPipeline}>
        <SelectTrigger>
          <SelectValue placeholder="Choose a pipeline" />
        </SelectTrigger>
        <SelectContent>
          {folder.pipelines.map(pipelineName => (
            <SelectItem key={pipelineName} value={pipelineName}>
              {pipelineName}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Label htmlFor="deployment-name">Name (optional)</Label>
      <Input id="deployment-name" value={name} onChange={event => setName(event.target.value)} placeholder={pipeline || 'feature'} />
    </FormDialog>
  )
}
