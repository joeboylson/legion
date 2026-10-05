// Everything the screen shows, kept current: loaded once, then again
// whenever legion2d reports a change.

import { useCallback, useEffect, useRef, useState } from 'react'

import type { Folder } from '@/generated/Folder'
import type { Run } from '@/generated/Run'
import type { SessionInfo } from '@/generated/SessionInfo'
import { askFor, onConnectionChange, onLegionEvent } from '@/lib/legion'
import type { RunSnapshot } from '@/lib/needs-you'

// Changes come in bursts (a handoff is several entries); load once per burst.
const RELOAD_DELAY_MS = 233

export type LegionData = {
  isConnected: boolean
  folders: Folder[]
  snapshots: RunSnapshot[]
  problem?: string
  // Bumps on every change, for views that load their own data.
  changeCount: number
  reload: () => void
}

const loadRunSnapshot = async (run: Run, allSessions: readonly SessionInfo[]): Promise<RunSnapshot> => {
  const sessions = allSessions.filter(session => session.run === run.id)
  const isOpen = run.closed_ms === null
  if (!isOpen) return { run, sessions, missions: [], openQuestions: [], suggestions: [] }
  const [missionsReply, questionsReply, suggestionsReply] = await Promise.all([
    askFor('missions', { type: 'mission_list', run: run.id }),
    askFor('entries', {
      type: 'log',
      run: run.id,
      filter: { mission: null, position: null, kinds: null, since_ms: null, open_questions: true },
    }),
    askFor('entries', {
      type: 'log',
      run: run.id,
      filter: { mission: null, position: null, kinds: ['suggestion'], since_ms: null, open_questions: false },
    }),
  ])
  return {
    run,
    sessions,
    missions: missionsReply.missions,
    openQuestions: questionsReply.entries,
    suggestions: suggestionsReply.entries,
  }
}

const loadEverything = async (): Promise<{ folders: Folder[]; snapshots: RunSnapshot[] }> => {
  const [foldersReply, runsReply, sessionsReply] = await Promise.all([
    askFor('folders', { type: 'folder_list' }),
    askFor('runs', { type: 'run_list', folder: null }),
    askFor('sessions', { type: 'session_list', run: null }),
  ])
  const snapshots = await Promise.all(runsReply.runs.map(run => loadRunSnapshot(run, sessionsReply.sessions)))
  return { folders: foldersReply.folders, snapshots }
}

export const useLegion = (): LegionData => {
  const [isConnected, setIsConnected] = useState(false)
  const [folders, setFolders] = useState<Folder[]>([])
  const [snapshots, setSnapshots] = useState<RunSnapshot[]>([])
  const [problem, setProblem] = useState<string>()
  const [changeCount, setChangeCount] = useState(0)
  const pendingReload = useRef<number | undefined>(undefined)

  const reload = useCallback(() => {
    window.clearTimeout(pendingReload.current)
    pendingReload.current = window.setTimeout(() => {
      loadEverything()
        .then(loaded => {
          setFolders(loaded.folders)
          setSnapshots(loaded.snapshots)
          setIsConnected(true)
          setProblem(undefined)
          setChangeCount(count => count + 1)
        })
        .catch((error: unknown) => {
          setIsConnected(false)
          setProblem(String(error))
        })
    }, RELOAD_DELAY_MS)
  }, [])

  useEffect(() => {
    reload()
    const stopEvents = onLegionEvent(() => reload())
    const stopConnection = onConnectionChange(connected => {
      setIsConnected(connected)
      if (connected) reload()
    })
    return () => {
      void stopEvents.then(stop => stop())
      void stopConnection.then(stop => stop())
    }
  }, [reload])

  return { isConnected, folders, snapshots, problem, changeCount, reload }
}
