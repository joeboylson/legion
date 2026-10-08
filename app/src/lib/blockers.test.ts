import { describe, expect, it } from 'vitest'

import { markedRow, markedRows, blockersIn, rowKeys } from './blockers'
import type { Escalation } from './escalations'

const item = (kind: Escalation['kind'], position = 'builder'): Escalation => ({
  key: `${kind}:d1:${position}`,
  folderPath: '/repo',
  kind,
  deploymentId: 'd1',
  deploymentName: 'feature',
  isClosed: false,
  position,
  text: 'waiting',
})

const openOnly =
  (...keys: string[]) =>
  (key: string) =>
    keys.includes(key)

const ALL_OPEN = ['folder:/repo', 'folder:/repo:deployments', 'deployment:d1', 'deployment:d1:assigned operators']

describe('blockersIn', () => {
  it('keeps what stops the work, not what clears by itself or is only advice', () => {
    const items = [item('stuck'), item('permission'), item('limit'), item('suggestion'), item('question')]
    expect(blockersIn([...items, item('halted')]).map(found => found.kind)).toEqual(['stuck', 'permission', 'halted'])
  })
})

describe('markedRow', () => {
  it('marks the folder when it is closed', () => {
    expect(markedRow(item('permission'), openOnly())).toBe('folder:/repo')
  })

  it('moves down one row for each open one', () => {
    expect(markedRow(item('permission'), openOnly('folder:/repo'))).toBe('folder:/repo:deployments')
    expect(markedRow(item('permission'), openOnly('folder:/repo', 'folder:/repo:deployments'))).toBe('deployment:d1')
    expect(markedRow(item('permission'), openOnly(...ALL_OPEN.slice(0, 3)))).toBe('deployment:d1:assigned operators')
  })

  it('marks the operator once everything above it is open', () => {
    expect(markedRow(item('permission'), openOnly(...ALL_OPEN))).toBe(rowKeys.operator('d1', 'builder'))
  })

  it('stops at the first closed row even when ones below are open', () => {
    expect(markedRow(item('stuck'), openOnly('folder:/repo', 'deployment:d1'))).toBe('folder:/repo:deployments')
  })
})

describe('markedRows', () => {
  it('gathers what each row carries', () => {
    const marks = markedRows([item('stuck', 'commander'), item('permission', 'builder'), item('limit')], openOnly())
    expect([...marks.keys()]).toEqual(['folder:/repo'])
    expect(marks.get('folder:/repo')?.map(found => found.position)).toEqual(['commander', 'builder'])
  })
})
