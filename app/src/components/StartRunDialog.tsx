// Starting a run: a folder, one of its pipelines, and an optional name.

import { useState } from 'react'

import { FormDialog } from '@/components/FormDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { Folder } from '@/generated/Folder'
import type { Run } from '@/generated/Run'
import { askFor } from '@/lib/legion'

type StartRunDialogProps = { folders: readonly Folder[]; onStarted: (run: Run) => void }

export function StartRunDialog({ folders, onStarted }: StartRunDialogProps) {
  const [folderPath, setFolderPath] = useState('')
  const [pipeline, setPipeline] = useState('')
  const [name, setName] = useState('')
  const pipelines = folders.find(folder => folder.path === folderPath)?.pipelines ?? []
  const isComplete = folderPath !== '' && pipeline !== ''
  return (
    <FormDialog
      trigger={<Button variant="ghost" size="xs" disabled={folders.length === 0}>Start run</Button>}
      title="Start a run"
      submitLabel="Start"
      canSubmit={isComplete}
      onSubmit={async () => {
        const trimmedName = name.trim()
        const { run } = await askFor('run', { type: 'run_start', folder: folderPath, pipeline, name: trimmedName === '' ? null : trimmedName })
        onStarted(run)
      }}
    >
      <Label>Folder</Label>
      <Select
        value={folderPath}
        onValueChange={path => {
          setFolderPath(path)
          setPipeline('')
        }}
      >
        <SelectTrigger>
          <SelectValue placeholder="Choose a folder" />
        </SelectTrigger>
        <SelectContent>
          {folders.map(folder => (
            <SelectItem key={folder.path} value={folder.path}>
              {folder.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Label>Pipeline</Label>
      <Select value={pipeline} onValueChange={setPipeline} disabled={pipelines.length === 0}>
        <SelectTrigger>
          <SelectValue placeholder="Choose a pipeline" />
        </SelectTrigger>
        <SelectContent>
          {pipelines.map(pipelineName => (
            <SelectItem key={pipelineName} value={pipelineName}>
              {pipelineName}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Label htmlFor="run-name">Name (optional)</Label>
      <Input id="run-name" value={name} onChange={event => setName(event.target.value)} placeholder={pipeline || 'feature'} />
    </FormDialog>
  )
}
