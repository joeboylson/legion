import { describe, expect, it } from 'vitest'

import { hashToRoute, type Route, routeToHash } from './route'

const roundTrip = (route: Route) => hashToRoute(routeToHash(route))

describe('routes', () => {
  it('bring back a deployment and its tab', () => {
    const route: Route = { selection: { deploymentId: 'a3da7e3a' }, deploymentTab: 'operators' }
    expect(routeToHash(route)).toBe('#/deployment/a3da7e3a/operators')
    expect(roundTrip(route)).toEqual(route)
  })

  it('bring back a folder whose path has slashes, and its tab', () => {
    const route: Route = { selection: { folderPath: '/Users/me/repo', folderTab: 'pipelines' }, deploymentTab: 'missions' }
    expect(roundTrip(route)).toEqual(route)
  })

  it('bring back a question', () => {
    const route: Route = { selection: { questionKey: 'question:42' }, deploymentTab: 'missions' }
    expect(roundTrip(route)).toEqual(route)
  })

  it('start fresh on an empty or unknown address', () => {
    const start = { selection: {}, deploymentTab: 'missions' }
    expect(hashToRoute('')).toEqual(start)
    expect(hashToRoute('#/nowhere/x')).toEqual(start)
    expect(hashToRoute('#/deployment/a3da7e3a/bogus').deploymentTab).toBe('missions')
  })
})
