import { describe, expect, it } from 'vitest'

import { parseOpenRows, withRowOpen } from './open-rows'

describe('open sidebar rows', () => {
  it('reads saved keys, or none when there are none or they are garbled', () => {
    expect([...parseOpenRows('["folder:/a","deployment:x"]')]).toEqual(['folder:/a', 'deployment:x'])
    expect(parseOpenRows(null).size).toBe(0)
    expect(parseOpenRows('{oops').size).toBe(0)
    expect(parseOpenRows('{"a":1}').size).toBe(0)
    expect([...parseOpenRows('["a",2]')]).toEqual(['a'])
  })

  it('opens and closes one row without touching the others', () => {
    const opened = withRowOpen(new Set(['a']), 'b', true)
    expect([...opened]).toEqual(['a', 'b'])
    expect([...withRowOpen(opened, 'a', false)]).toEqual(['b'])
  })
})
