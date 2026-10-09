// An action is what the CLI calls a command and its action word: a list of
// steps that each fill one answer, then a run that sends them to legion2d.
// Steps already answered (by what was clicked, or by the open tab) are
// skipped, the way a flag skips a prompt.

import type { Channels } from '@/generated/Channels'
import type { Folder } from '@/generated/Folder'
import type { FolderDetail } from '@/generated/FolderDetail'
import type { DeploymentSnapshot } from '@/lib/escalations'
import type { DeploymentPart, FolderTab } from '@/lib/tabs'

export type Answer = string | readonly string[]
export type Answers = Readonly<Record<string, Answer>>

// The keys a click or the open tab can fill in.
export type Prefill = Partial<Record<'folder' | 'deployment' | 'mission' | 'operator' | 'pipeline' | 'position' | 'question' | 'address', string>>

// Where the app can go, for actions that open what they made.
export type GoTo = {
  folder: (path: string, tab?: FolderTab) => void
  deploymentPart: (deploymentId: string, part: DeploymentPart) => void
  mission: (deploymentId: string, number: number) => void
  session: (deploymentId: string, position: string) => void
  blockers: () => void
  channels: () => void
}

// What steps read from: what the app already holds, a folder's setup, and
// where to go.
export type Sources = {
  folders: readonly Folder[]
  snapshots: readonly DeploymentSnapshot[]
  channels?: Channels
  folderDetail: (path: string) => Promise<FolderDetail>
  goTo: GoTo
}

export type Choice = { value: string; label: string; hint?: string }

type StepBase = {
  key: string
  prompt: string | ((answers: Answers, sources: Sources) => string)
  // Asked only when this holds.
  when?: (answers: Answers) => boolean
}

export type Step = StepBase &
  (
    | { kind: 'text'; placeholder?: string; isOptional?: boolean; initial?: (answers: Answers, sources: Sources) => Promise<string> | string }
    // Typed or from a file; with `initial`, it opens ready to edit.
    | { kind: 'longText'; placeholder?: string; initial?: (answers: Answers, sources: Sources) => Promise<string> | string }
    | { kind: 'pick'; choices: (answers: Answers, sources: Sources) => Promise<Choice[]> | Choice[]; empty: string }
    | { kind: 'pickMany'; choices: (answers: Answers, sources: Sources) => Promise<Choice[]> | Choice[]; empty: string }
    | { kind: 'pickInOrder'; choices: (answers: Answers, sources: Sources) => Promise<Choice[]> | Choice[]; empty: string }
    | { kind: 'confirm'; yesLabel: string }
  )

// The things an action can be about: its row's menu lists it.
export type Subject = 'app' | 'folder' | 'operator' | 'pipeline' | 'deployment' | 'closedDeployment' | 'mission' | 'question' | 'session' | 'channel'

export type Action = {
  id: string
  // How the command menu groups it, like the CLI's command.
  group: string
  label: string
  subjects: readonly Subject[]
  isDestructive?: boolean
  steps: readonly Step[]
  // Resolves with what to tell the admin, or throws why it couldn't.
  run: (answers: Answers, sources: Sources) => Promise<string>
}
