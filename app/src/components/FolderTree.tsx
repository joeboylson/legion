// This machine's folders and their runs. Another machine's come with step 6.

import type { Folder } from '@/generated/Folder'
import type { Run } from '@/generated/Run'
import type { RunSnapshot } from '@/lib/needs-you'

type FolderTreeProps = {
  folders: readonly Folder[]
  snapshots: readonly RunSnapshot[]
  selectedRunId?: string
  onSelectRun: (run: Run) => void
}

const isWorking = (snapshot: RunSnapshot): boolean => snapshot.sessions.some(session => session.activity === 'busy')

export function FolderTree({ folders, snapshots, selectedRunId, onSelectRun }: FolderTreeProps) {
  return (
    <ul className="tree" role="tree" aria-label="Folders and runs">
      <li role="none">
        <span className="label flex h-row items-center px-4">This machine</span>
      </li>
      {folders.map(folder => {
        const folderRuns = snapshots.filter(snapshot => snapshot.run.folder === folder.path && snapshot.run.closed_ms === null)
        return (
          <li key={folder.path} role="none">
            <span className="flex h-row items-center px-5 font-medium" title={folder.path}>
              {folder.name}
            </span>
            <ul className="tree" role="group">
              {folderRuns.map(snapshot => (
                <li key={snapshot.run.id} role="none">
                  <button
                    type="button"
                    role="treeitem"
                    style={{ '--depth': 2 } as React.CSSProperties}
                    aria-selected={snapshot.run.id === selectedRunId}
                    onClick={() => onSelectRun(snapshot.run)}
                  >
                    <span className="dot" style={{ color: isWorking(snapshot) ? 'var(--success)' : 'var(--fg-muted)' }} />
                    {snapshot.run.name}
                    <span className="muted">{snapshot.run.pipeline}</span>
                  </button>
                </li>
              ))}
              {folderRuns.length === 0 && (
                <li role="none" className="px-6 text-muted-foreground">
                  no open runs
                </li>
              )}
            </ul>
          </li>
        )
      })}
    </ul>
  )
}
