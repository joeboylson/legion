// The deployment's missions and where each stands. Click one to read it.

import { MissionDialog } from '@/components/MissionDialog'
import type { Mission } from '@/generated/Mission'
import { MISSION_STATUS_LABELS } from '@/lib/format'

export function MissionsTab({ missions }: { missions: readonly Mission[] }) {
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
              <MissionDialog mission={mission} />
            </td>
            <td>{MISSION_STATUS_LABELS[mission.status]}</td>
            <td className="font-mono">{mission.holder ?? ''}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
