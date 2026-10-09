// Moving through an action's steps: which comes next, and going back. Every
// answer remembers how it came, so going back skips over what was filled in
// for the admin and stops at what they chose.

import type { Action, Answer, Answers, Prefill, Sources, Step } from './types'

export type How = 'given' | 'auto' | 'chosen'
// `label` is how a picked value reads, when it isn't the value itself.
export type Filled = { key: string; value: Answer; how: How; label?: string }

export const answersOf = (filled: readonly Filled[]): Answers => Object.fromEntries(filled.map(({ key, value }) => [key, value]))

export const isAnswered = (filled: readonly Filled[], key: string) => filled.some(entry => entry.key === key)

// The first step still to ask, or none once every step that applies is filled.
export const nextStep = (action: Action, filled: readonly Filled[]): Step | undefined => {
  const answers = answersOf(filled)
  return action.steps.find(step => !isAnswered(filled, step.key) && (step.when?.(answers) ?? true))
}

export const withAnswer = (filled: readonly Filled[], key: string, value: Answer, how: How, label?: string): Filled[] => [
  ...filled.filter(entry => entry.key !== key),
  { key, value, how, ...(label === undefined ? {} : { label }) },
]

// Back to one step: its answer and every one after it go, except what was given.
export const backTo = (filled: readonly Filled[], key: string): Filled[] => {
  const index = filled.findIndex(entry => entry.key === key)
  if (index === -1) return [...filled]
  return filled.filter((entry, at) => at < index || entry.how === 'given')
}

// The steps that apply so far, in order: a step whose `when` depends on a
// later answer shows once that answer is in.
export const stepsShown = (action: Action, filled: readonly Filled[]): readonly Step[] => {
  const answers = answersOf(filled)
  return action.steps.filter(step => step.when?.(answers) ?? true)
}

// Back to the last answer the admin chose, dropping it and anything filled
// in after it. What was given never goes.
export const back = (filled: readonly Filled[]): Filled[] => {
  const lastChosen = filled.findLastIndex(entry => entry.how === 'chosen')
  if (lastChosen === -1) return [...filled]
  return filled.filter((entry, index) => index < lastChosen || entry.how === 'given')
}

export const canGoBack = (filled: readonly Filled[]) => filled.some(entry => entry.how === 'chosen')

export const promptOf = (step: Step, answers: Answers, sources: Sources) => (typeof step.prompt === 'string' ? step.prompt : step.prompt(answers, sources))

export const textOf = (answer: Answer | undefined): string => (typeof answer === 'string' ? answer : '')
export const listOf = (answer: Answer | undefined): readonly string[] => (Array.isArray(answer) ? answer : [])

// What a click or the open tab filled in, as answers that are never undone.
export const givenFrom = (prefill: Prefill): Filled[] =>
  Object.entries(prefill)
    .filter((entry): entry is [string, string] => entry[1] !== undefined && entry[1] !== '')
    .map(([key, value]) => ({ key, value, how: 'given' }))
