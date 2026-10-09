// The deployment's missions and where each stands. Click one to read it;
// right-click it, or its "…", to pause, resume or finish it.

import { ActionContextMenu, ActionMenuButton } from '@/components/actions/ActionMenus'
import type { Mission } from '@/generated/Mission'
import { MISSION_STATUS_LABELS } from '@/lib/format'

type MissionsTabProps = { deploymentId: string; missions: readonly Mission[]; onOpen: (mission: Mission) => void }

export function MissionsTab({ deploymentId, missions, onOpen }: MissionsTabProps) {
  if (missions.length === 0) return <p className="text-muted-foreground">No missions yet.</p>
  return (
    <table>
      <thead>
        <tr>
          <th>#</th>
          <th>Mission</th>
          <th>Where it stands</th>
          <th>Held by</th>
          <th aria-label="Actions" />
        </tr>
      </thead>
      <tbody>
        {missions.map(mission => {
          const prefill = { deployment: deploymentId, mission: String(mission.number) }
          const read = { label: 'Read it', onOpen: () => onOpen(mission) }
          return (
            <ActionContextMenu key={mission.number} subject="mission" prefill={prefill} open={read}>
              <tr>
                <td className="font-mono">{mission.number}</td>
                <td>
                  <button type="button" className="text-left underline-offset-4 hover:underline" onClick={() => onOpen(mission)}>
                    {mission.title}
                  </button>
                </td>
                <td>{MISSION_STATUS_LABELS[mission.status]}</td>
                <td className="font-mono">{mission.holder ?? ''}</td>
                <td>
                  <ActionMenuButton subject="mission" prefill={prefill} open={read} label={`Mission ${mission.number}`} />
                </td>
              </tr>
            </ActionContextMenu>
          )
        })}
      </tbody>
    </table>
  )
}
