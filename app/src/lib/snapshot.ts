// Debug builds with LEGION2_SNAPSHOT_DIR set: after the screen settles,
// save the page as rendered and where every element sits, so the layout can
// be checked without a person looking.

import { invoke } from '@tauri-apps/api/core'

import { isInAppWindow } from './legion-connection'

const SETTLE_DELAY_MS = 610
const TEXT_PREVIEW_LENGTH = 55

export type ElementBox = {
  path: string
  text: string
  x: number
  y: number
  width: number
  height: number
}

const describeElement = (element: Element): string => {
  const classes = [...element.classList].slice(0, 3).map(name => `.${name}`).join('')
  return `${element.tagName.toLowerCase()}${classes}`
}

const elementPath = (element: Element): string => {
  const ancestors: Element[] = []
  for (let current: Element | null = element; current !== null && current !== document.body; current = current.parentElement) {
    ancestors.unshift(current)
  }
  return ancestors.map(describeElement).join(' > ')
}

const ownText = (element: Element): string =>
  [...element.childNodes]
    .filter(node => node.nodeType === Node.TEXT_NODE)
    .map(node => node.textContent ?? '')
    .join('')
    .trim()
    .slice(0, TEXT_PREVIEW_LENGTH)

export const elementBoxes = (root: Element): ElementBox[] =>
  [...root.querySelectorAll('*')]
    .map(element => ({ element, rect: element.getBoundingClientRect() }))
    .filter(({ rect }) => rect.width > 0 && rect.height > 0)
    .map(({ element, rect }) => ({
      path: elementPath(element),
      text: ownText(element),
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    }))

// A copied canvas is blank, so each becomes a picture of what it shows.
const canvasesAsImages = (original: HTMLElement, copy: HTMLElement) => {
  const drawn = [...original.querySelectorAll('canvas')]
  copy.querySelectorAll('canvas').forEach((canvas, index) => {
    const source = drawn[index]
    if (source === undefined) return
    const picture = document.createElement('img')
    picture.src = source.toDataURL()
    picture.style.cssText = canvas.style.cssText
    picture.style.width = `${source.clientWidth}px`
    picture.style.height = `${source.clientHeight}px`
    canvas.replaceWith(picture)
  })
}

// The page without its scripts: a browser redraws it from the HTML alone.
const pageWithoutScripts = (): string => {
  const copy = document.documentElement.cloneNode(true) as HTMLElement
  copy.querySelectorAll('script').forEach(script => script.remove())
  canvasesAsImages(document.documentElement, copy)
  return `<!doctype html>\n${copy.outerHTML}`
}

const saveSnapshot = async (): Promise<void> => {
  const layout = { width: window.innerWidth, height: window.innerHeight, elements: elementBoxes(document.body) }
  await invoke('save_snapshot', { page: pageWithoutScripts(), layout: JSON.stringify(layout, null, 1) })
}

export type StartingView = { deployment: string; position: string | null }

// Debug builds: the deployment (and position) LEGION2_OPEN names, to open at start.
// Only the app's window has a back end to ask; a browser tab opens as usual.
export const readStartingView = async (): Promise<StartingView | undefined> =>
  isInAppWindow() ? ((await invoke<StartingView | null>('starting_view')) ?? undefined) : undefined

export const startSnapshots = async (): Promise<void> => {
  const isSnapshotting = isInAppWindow() && (await invoke<boolean>('is_snapshotting'))
  if (!isSnapshotting) return
  let pending: number | undefined
  const saveSoon = () => {
    window.clearTimeout(pending)
    pending = window.setTimeout(() => void saveSnapshot(), SETTLE_DELAY_MS)
  }
  new MutationObserver(saveSoon).observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true })
  window.addEventListener('resize', saveSoon)
  saveSoon()
}
