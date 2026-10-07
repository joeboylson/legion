import { describe, expect, it } from 'vitest'

import { socketAddress } from './legion-browser'

describe('socketAddress', () => {
  it('connects back to the address that served the page', () => {
    expect(socketAddress({ host: '127.0.0.1:4610' })).toBe('ws://127.0.0.1:4610/ws')
    expect(socketAddress({ host: 'localhost:4610' })).toBe('ws://localhost:4610/ws')
  })
})
