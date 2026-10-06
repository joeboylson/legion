// A crown beside the commander's name, wherever positions are listed.

import { Crown } from 'lucide-react'

import { COMMANDER } from '@/lib/roster'

export function CommanderCrown({ position }: { position: string }) {
  if (position !== COMMANDER) return null
  return <Crown className="size-4 flex-none text-muted-foreground" aria-label="leads the deployment" />
}
