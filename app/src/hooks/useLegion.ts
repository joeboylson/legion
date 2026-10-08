// Everything the screen shows, kept current: loaded once, then again
// whenever legion2d reports a change.

import { useCallback, useEffect, useRef, useState } from 'react'

import type { Folder } from '@/generated/Folder'
import type { Deployment } from '@/generated/Deployment'
import type { PipelineDetail } from '@/generated/PipelineDetail'
import type { SessionInfo } from '@/generated/SessionInfo'
import { askFor, onConnectionChange, onLegionEvent } from '@/lib/legion'
import type { DeploymentSnapshot } from '@/lib/escalations'

// Changes come in bursts (a handoff is several entries); load once per burst.
const RELOAD_DELAY_MS = 233

export type LegionData = {
  isConnected: boolean
  folders: Folder[]
  snapshots: DeploymentSnapshot[]
  problem?: string
  // Bumps on every change, for views that load their own data.
  changeCount: number
  reload: () => void
}

// Each pipeline, by folder and then pipeline name.
type Pipelines = ReadonlyMap<string, ReadonlyMap<string, PipelineDetail>>

const loadPipelines = async (folderPaths: readonly string[]): Promise<Pipelines> => {
  const details = await Promise.all(folderPaths.map(folder => askFor('folder_detail', { type: 'folder_read', folder })))
  return new Map(details.map(({ detail }) => [detail.folder.path, new Map(detail.pipelines.map(pipeline => [pipeline.name, pipeline]))]))
}

const loadDeploymentSnapshot = async (
  deployment: Deployment,
  allSessions: readonly SessionInfo[],
  pipelines: Pipelines,
): Promise<DeploymentSnapshot> => {
  const sessions = allSessions.filter(session => session.deployment === deployment.id)
  const pipeline = pipelines.get(deployment.folder)?.get(deployment.pipeline)
  const isOpen = deployment.closed_ms === null
  // Closed: its missions, and the questions left unanswered when it closed,
  // are still worth reading; its suggestions went to a commander that's gone.
  const [missionsReply, questionsReply, suggestionsReply] = await Promise.all([
    askFor('missions', { type: 'mission_list', deployment: deployment.id }),
    askFor('entries', {
      type: 'log',
      deployment: deployment.id,
      filter: { mission: null, position: null, kinds: null, since_ms: null, open_questions: true },
    }),
    isOpen
      ? askFor('entries', {
          type: 'log',
          deployment: deployment.id,
          filter: { mission: null, position: null, kinds: ['suggestion'], since_ms: null, open_questions: false },
        })
      : { entries: [] },
  ])
  return {
    deployment,
    sessions,
    pipelineOperators: pipeline?.operators ?? [],
    pipelineSteps: pipeline?.decisions ?? [],
    missions: missionsReply.missions,
    openQuestions: questionsReply.entries,
    suggestions: suggestionsReply.entries,
  }
}

const loadEverything = async (): Promise<{ folders: Folder[]; snapshots: DeploymentSnapshot[] }> => {
  const [foldersReply, deploymentsReply, sessionsReply] = await Promise.all([
    askFor('folders', { type: 'folder_list' }),
    askFor('deployments', { type: 'deployment_list', folder: null }),
    askFor('sessions', { type: 'session_list', deployment: null }),
  ])
  const pipelines = await loadPipelines([...new Set(deploymentsReply.deployments.map(deployment => deployment.folder))])
  const snapshots = await Promise.all(
    deploymentsReply.deployments.map(deployment => loadDeploymentSnapshot(deployment, sessionsReply.sessions, pipelines)),
  )
  return { folders: foldersReply.folders, snapshots }
}

export const useLegion = (): LegionData => {
  const [isConnected, setIsConnected] = useState(false)
  const [folders, setFolders] = useState<Folder[]>([])
  const [snapshots, setSnapshots] = useState<DeploymentSnapshot[]>([])
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
