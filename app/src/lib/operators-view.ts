// Which way the Operators tab shows its operators (or their log),
// remembered per viewer.

export const OPERATORS_VIEWS = ['graph', 'list', 'grid', 'log'] as const
export type OperatorsView = (typeof OPERATORS_VIEWS)[number]

const STORAGE_KEY = 'legion.operatorsView'
const DEFAULT_VIEW: OperatorsView = 'graph'

export const isOperatorsView = (value: string): value is OperatorsView => OPERATORS_VIEWS.some(view => view === value)

// Storage can be missing or refuse (a private window); the default stands in.
export const readOperatorsView = (): OperatorsView => {
  try {
    const saved = localStorage.getItem(STORAGE_KEY) ?? ''
    return isOperatorsView(saved) ? saved : DEFAULT_VIEW
  } catch {
    return DEFAULT_VIEW
  }
}

export const saveOperatorsView = (view: OperatorsView) => {
  try {
    localStorage.setItem(STORAGE_KEY, view)
  } catch {
    // Not remembered this time; nothing else depends on it.
  }
}
