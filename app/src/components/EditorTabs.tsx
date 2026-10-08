// The strip of open pages along the top of the main area, as in an editor.
// Click one to show it; its cross, or a middle click, closes it.

import { X } from 'lucide-react'
import type { KeyboardEvent, MouseEvent } from 'react'

import { Button } from '@/components/ui/button'

export type EditorTabItem = { key: string; label: string; title: string }

type EditorTabsProps = {
  items: readonly EditorTabItem[]
  activeKey?: string
  onActivate: (key: string) => void
  onClose: (key: string) => void
}

const MIDDLE_BUTTON = 1

export function EditorTabs({ items, activeKey, onActivate, onClose }: EditorTabsProps) {
  const closeOnMiddleClick = (key: string) => (event: MouseEvent) => {
    if (event.button !== MIDDLE_BUTTON) return
    event.preventDefault()
    onClose(key)
  }
  const activateOnKey = (key: string) => (event: KeyboardEvent) => {
    if (event.key !== 'Enter' && event.key !== ' ') return
    event.preventDefault()
    onActivate(key)
  }
  return (
    <div role="tablist" aria-label="Open pages" className="tabs h-[var(--bar)] flex-none">
      {items.map(item => (
        <div
          key={item.key}
          role="tab"
          tabIndex={0}
          aria-selected={item.key === activeKey}
          title={item.title}
          className="tab max-w-[var(--sidebar)] pr-1"
          onClick={() => onActivate(item.key)}
          onKeyDown={activateOnKey(item.key)}
          onAuxClick={closeOnMiddleClick(item.key)}
        >
          <span className="truncate">{item.label}</span>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`Close ${item.label}`}
            title="Close"
            onClick={event => {
              event.stopPropagation()
              onClose(item.key)
            }}
          >
            <X className="size-4" />
          </Button>
        </div>
      ))}
    </div>
  )
}
