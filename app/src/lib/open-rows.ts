// Which sidebar rows are open, kept as a set of row keys so a reload comes
// back with the same rows open.

// A saved list of keys, or none open when there's none or it can't be read.
export const parseOpenRows = (saved: string | null): ReadonlySet<string> => {
  if (saved === null) return new Set()
  try {
    const keys: unknown = JSON.parse(saved)
    return Array.isArray(keys) ? new Set(keys.filter((key): key is string => typeof key === 'string')) : new Set()
  } catch {
    return new Set()
  }
}

export const withRowOpen = (openRows: ReadonlySet<string>, key: string, isOpen: boolean): ReadonlySet<string> =>
  isOpen ? new Set([...openRows, key]) : new Set([...openRows].filter(openKey => openKey !== key))
