// The deployment's missions and where each stands. Click one to read it.

import type { Mission } from '@/generated/Mission'
import { MISSION_STATUS_LABELS } from '@/lib/format'

type MissionsTabProps = { missions: readonly Mission[]; onOpen: (mission: Mission) => void }

export function MissionsTab({ missions, onOpen }: MissionsTabProps) {
  if (missions.length === 0) return <p className="text-muted-foreground">No missions yet.</p>
  return (
    <table>
      <thead>
        <tr>
          <th>#</th>
          <th>Mission</th>
          <th>Where it stands</th>
          <th>Held by</th>
        </tr>
      </thead>
      <tbody>
        {missions.map(mission => (
          <tr key={mission.number}>
            <td className="font-mono">{mission.number}</td>
            <td>
              <button type="button" className="text-left underline-offset-4 hover:underline" onClick={() => onOpen(mission)}>
                {mission.title}
              </button>
            </td>
            <td>{MISSION_STATUS_LABELS[mission.status]}</td>
            <td className="font-mono">{mission.holder ?? ''}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
