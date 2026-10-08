import { describe, expect, it } from 'vitest'

import { PARTY_PALETTE, partyColors, UNKNOWN_PARTY_COLOR } from './party-colors'

describe('partyColors', () => {
  it('gives each team its own color, the same whatever order they come in', () => {
    const colors = partyColors(['laptop/a', 'laptop/b', 'laptop', 'laptop/a'])
    const again = partyColors(['laptop', 'laptop/b', 'laptop/a'])
    expect(new Set(['laptop/a', 'laptop/b', 'laptop'].map(colors)).size).toBe(3)
    expect(['laptop/a', 'laptop/b'].map(colors)).toEqual(['laptop/a', 'laptop/b'].map(again))
  })

  it("reuses the palette past its size, and greys out what it doesn't know", () => {
    const many = Array.from({ length: PARTY_PALETTE.length + 1 }, (_, index) => `p${String(index).padStart(2, '0')}`)
    const colors = partyColors(many)
    expect(colors(`p${String(PARTY_PALETTE.length).padStart(2, '0')}`)).toBe(colors('p00'))
    expect(colors('elsewhere')).toBe(UNKNOWN_PARTY_COLOR)
    expect(colors(null)).toBe(UNKNOWN_PARTY_COLOR)
  })
})
