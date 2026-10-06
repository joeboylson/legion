// Name and value rows, names in one column and values in the next.

import type { ReactNode } from 'react'

export function Setting({ name, value }: { name: string; value: string | null }) {
  return (
    <>
      <dt className="text-muted-foreground">{name}</dt>
      <dd className="m-0 font-mono break-all">{value ?? '—'}</dd>
    </>
  )
}

export function SettingsList({ children }: { children: ReactNode }) {
  return <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">{children}</dl>
}
