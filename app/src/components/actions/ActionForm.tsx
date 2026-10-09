// One action's steps, asked one at a time inside the command menu's box:
// lists stay searchable, texts are typed or read from a file. A trail along
// the top says where you are; Backspace on an empty box goes back a step.

import { Check } from 'lucide-react'
import { type KeyboardEvent, type ReactNode, useCallback, useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '@/components/ui/command'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { answersOf, back, backTo, canGoBack, type Filled, nextStep, promptOf, stepsShown, withAnswer } from '@/lib/actions/flow'
import type { Action, Answers, Choice, Sources, Step } from '@/lib/actions/types'
import { cn } from '@/lib/utils'

// `initialFilled` is what was filled in before it opened; it's keyed per
// opening, so it's only read once.
type ActionFormProps = { action: Action; initialFilled: readonly Filled[]; sources: Sources; onDone: (message: string) => void }

// How a long text arrives, chosen first unless it's being edited.
type TextSource = 'typed' | 'file'

const isSubmitKey = (event: KeyboardEvent) => event.key === 'Enter' && (event.metaKey || event.ctrlKey)

// Puts the caret in a step's box as it appears.
const focus = (element: HTMLElement | null) => element?.focus()

// The command menu takes Enter and the arrows for its lists; a typing step
// keeps its keys to itself.
const keepKeys = (event: KeyboardEvent) => event.stopPropagation()

// A step's title before it's reached: its prompt, or its name when the
// prompt needs answers it doesn't have yet.
const stepTitle = (step: Step, answers: Answers, sources: Sources, isReached: boolean) => {
  if (typeof step.prompt === 'string' || isReached) return promptOf(step, answers, sources)
  return step.key.charAt(0).toUpperCase() + step.key.slice(1)
}

// How an answer reads in the list: what was picked, or the first line typed.
const answerText = (entry: Filled, sources: Sources): string => {
  if (entry.label !== undefined) return entry.label
  if (Array.isArray(entry.value)) return entry.value.join(', ')
  const value = typeof entry.value === 'string' ? entry.value : ''
  if (entry.key === 'deployment') return sources.snapshots.find(snapshot => snapshot.deployment.id === value)?.deployment.name ?? value
  if (entry.key === 'folder') return value.split('/').filter(Boolean).at(-1) ?? value
  if (entry.key === 'confirm') return 'yes'
  return value.split('\n')[0] ?? ''
}

type StepRowProps = { number: number; title: string; entry?: Filled; isCurrent: boolean; sources: Sources; onReopen: () => void; children?: ReactNode }

// One step in the list: done (click to change it), open, or still to come.
function StepRow({ number, title, entry, isCurrent, sources, onReopen, children }: StepRowProps) {
  const canReopen = entry !== undefined && entry.how !== 'given'
  return (
    <li className={cn('flex flex-col border-b border-border', !isCurrent && entry === undefined && 'text-muted-foreground opacity-60')}>
      <button
        type="button"
        disabled={!canReopen}
        onClick={onReopen}
        title={canReopen ? 'Change this answer' : undefined}
        className={cn('flex items-baseline gap-3 px-3 py-2 text-left', canReopen && 'hover:bg-layer-2', isCurrent && 'font-medium')}
      >
        <span className="w-5 flex-none font-mono text-label text-muted-foreground">{entry === undefined ? number : <Check className="size-4" />}</span>
        <span className="flex-none">{title}</span>
        {entry !== undefined && <span className="ml-auto truncate font-mono text-label text-muted-foreground">{answerText(entry, sources)}</span>}
      </button>
      {isCurrent && children}
    </li>
  )
}

type StepProps = { step: Step; prompt: string; choices?: readonly Choice[]; initial?: string; onAnswer: (value: string | readonly string[]) => void; onBack: () => void }

function TextStep({ prompt, initial, placeholder, isOptional, onAnswer, onBack }: StepProps & { placeholder?: string; isOptional?: boolean }) {
  const [value, setValue] = useState(initial ?? '')
  const canSubmit = isOptional === true || value.trim() !== ''
  return (
    <form
      className="flex flex-col gap-2 p-3"
      onKeyDown={keepKeys}
      onSubmit={event => {
        event.preventDefault()
        if (canSubmit) onAnswer(value)
      }}
    >
      <label className="sr-only" htmlFor="action-text">
        {prompt}
      </label>
      <Input
        id="action-text"
        value={value}
        placeholder={placeholder}
        ref={focus}
        onChange={event => setValue(event.target.value)}
        onKeyDown={event => {
          if (event.key === 'Backspace' && value === '') onBack()
        }}
      />
      <span className="text-label text-muted-foreground">Enter to go on{isOptional === true ? ', blank to leave it' : ''}</span>
    </form>
  )
}

function LongTextStep({ prompt, initial, placeholder, onAnswer, onBack }: StepProps & { placeholder?: string }) {
  const isEditing = initial !== undefined && initial !== ''
  const [source, setSource] = useState<TextSource | undefined>(isEditing ? 'typed' : undefined)
  const [value, setValue] = useState(initial ?? '')
  const [problem, setProblem] = useState<string>()
  const readFile = async (file: File) => {
    try {
      onAnswer(await file.text())
    } catch (error) {
      setProblem(String(error))
    }
  }
  if (source === undefined) {
    return (
      <PickList
        prompt={prompt}
        choices={[
          { value: 'typed', label: 'Type it', hint: '⌘ Enter to finish' },
          { value: 'file', label: 'From a file', hint: 'pick one from your computer' },
        ]}
        onPick={picked => setSource(picked === 'file' ? 'file' : 'typed')}
        onBack={onBack}
      />
    )
  }
  if (source === 'file') {
    return (
      <div className="flex flex-col gap-2 p-3" onKeyDown={keepKeys}>
        <label className="sr-only" htmlFor="action-file">
          {prompt}
        </label>
        <input
          id="action-file"
          type="file"
          ref={focus}
          onChange={event => {
            const file = event.target.files?.[0]
            if (file !== undefined) void readFile(file)
          }}
        />
        {problem !== undefined && <p className="m-0 text-danger">{problem}</p>}
        <Button type="button" variant="ghost" className="self-start" onClick={() => setSource(undefined)}>
          Type it instead
        </Button>
      </div>
    )
  }
  return (
    <div className="flex flex-col gap-2 p-3" onKeyDown={keepKeys}>
      <label className="sr-only" htmlFor="action-long-text">
        {prompt}
      </label>
      <Textarea
        id="action-long-text"
        className={cn('min-h-8 font-mono', isEditing && 'min-h-[var(--panel)]')}
        value={value}
        placeholder={placeholder}
        ref={focus}
        onChange={event => setValue(event.target.value)}
        onKeyDown={event => {
          if (isSubmitKey(event) && value.trim() !== '') onAnswer(value)
          if (event.key === 'Backspace' && value === '') onBack()
        }}
      />
      <div className="flex items-center gap-3">
        <Button type="button" disabled={value.trim() === ''} onClick={() => onAnswer(value)}>
          Continue
        </Button>
        <span className="text-label text-muted-foreground">or ⌘ Enter</span>
      </div>
    </div>
  )
}

type PickListProps = { prompt: string; choices: readonly Choice[]; empty?: string; onPick: (value: string) => void; onBack: () => void; marked?: readonly string[]; extra?: Choice }

// A searchable list; Backspace in an empty search goes back.
function PickList({ prompt, choices, empty, onPick, onBack, marked = [], extra }: PickListProps) {
  const [search, setSearch] = useState('')
  const markedSet = new Set(marked)
  return (
    <>
      <CommandInput
        placeholder={prompt}
        value={search}
        onValueChange={setSearch}
        ref={focus}
        onKeyDown={event => {
          if (event.key === 'Backspace' && search === '') onBack()
        }}
      />
      <CommandList>
        <CommandEmpty>{choices.length === 0 ? empty : 'Nothing matches.'}</CommandEmpty>
        <CommandGroup>
          {extra !== undefined && (
            <CommandItem value={`${extra.label} ${extra.value}`} onSelect={() => onPick(extra.value)}>
              <span className="font-medium">{extra.label}</span>
              {extra.hint !== undefined && <span className="ml-auto truncate text-label text-muted-foreground">{extra.hint}</span>}
            </CommandItem>
          )}
          {choices.map(choice => (
            <CommandItem key={choice.value} value={`${choice.label} ${choice.value}`} onSelect={() => onPick(choice.value)}>
              <Check className={cn('size-4', markedSet.has(choice.value) ? 'opacity-100' : 'opacity-0')} />
              <span className="truncate">{choice.label}</span>
              {choice.hint !== undefined && <span className="ml-auto truncate text-label text-muted-foreground">{choice.hint}</span>}
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </>
  )
}

function PickManyStep({ prompt, choices = [], onAnswer, onBack }: StepProps) {
  const [ticked, setTicked] = useState<readonly string[]>(() => choices.map(choice => choice.value))
  const toggle = (value: string) => setTicked(current => (current.includes(value) ? current.filter(item => item !== value) : [...current, value]))
  return (
    <PickList
      prompt={`${prompt} (Enter ticks or unticks)`}
      choices={choices}
      marked={ticked}
      extra={{ value: '\u0000done', label: `Continue with ${ticked.length}`, hint: ticked.join(', ') }}
      onPick={value => (value === '\u0000done' ? ticked.length > 0 && onAnswer(ticked) : toggle(value))}
      onBack={onBack}
    />
  )
}

function PickInOrderStep({ prompt, choices = [], onAnswer, onBack }: StepProps) {
  const [chosen, setChosen] = useState<readonly string[]>([])
  const chosenSet = new Set(chosen)
  const left = choices.filter(choice => !chosenSet.has(choice.value))
  return (
    <PickList
      prompt={`${prompt}: step ${chosen.length + 1}`}
      choices={left}
      extra={chosen.length === 0 ? undefined : { value: '\u0000done', label: 'That’s all', hint: chosen.join(' → ') }}
      onPick={value => {
        if (value === '\u0000done') return onAnswer(chosen)
        const next = [...chosen, value]
        if (next.length === choices.length) return onAnswer(next)
        setChosen(next)
      }}
      onBack={() => (chosen.length === 0 ? onBack() : setChosen(chosen.slice(0, -1)))}
    />
  )
}

// Loads a step's list or starting text, which may need legion2d. A list
// with one choice is picked for you, as in the CLI.
function useStepData(step: Step | undefined, filled: readonly Filled[], sources: Sources, onOnlyChoice: (filledNow: readonly Filled[], key: string, value: string, label: string) => void) {
  const [data, setData] = useState<{ key: string; choices?: Choice[]; initial?: string; problem?: string }>()
  useEffect(() => {
    if (step === undefined) return
    const answers = answersOf(filled)
    const load = async () => {
      switch (step.kind) {
        case 'pick':
        case 'pickMany':
        case 'pickInOrder':
          return { key: step.key, choices: await step.choices(answers, sources) }
        case 'text':
        case 'longText':
          return { key: step.key, initial: step.initial === undefined ? undefined : await step.initial(answers, sources) }
        case 'confirm':
          return { key: step.key }
      }
    }
    let isCurrent = true
    const show = async () => {
      try {
        const loaded = await load()
        const only = step.kind === 'pick' && loaded.choices?.length === 1 ? loaded.choices[0] : undefined
        if (!isCurrent) return
        if (only !== undefined) return onOnlyChoice(filled, step.key, only.value, only.label)
        setData(loaded)
      } catch (error) {
        if (isCurrent) setData({ key: step.key, problem: String(error) })
      }
    }
    void show()
    return () => {
      isCurrent = false
    }
  }, [step, filled, sources, onOnlyChoice])
  return data?.key === step?.key ? data : undefined
}

export function ActionForm({ action, initialFilled, sources, onDone }: ActionFormProps) {
  const [filled, setFilled] = useState<readonly Filled[]>(initialFilled)
  const [problem, setProblem] = useState<string>()
  const [isRunning, setIsRunning] = useState(false)
  const step = nextStep(action, filled)
  const answers = answersOf(filled)

  const run = useCallback(
    async (complete: readonly Filled[]) => {
      setIsRunning(true)
      try {
        onDone(await action.run(answersOf(complete), sources))
      } catch (error) {
        setProblem(error instanceof Error ? error.message : String(error))
        setIsRunning(false)
      }
    },
    [action, sources, onDone],
  )
  // Each answer moves on; the last one runs the action.
  const advance = useCallback(
    (next: readonly Filled[]) => {
      setProblem(undefined)
      setFilled(next)
      if (nextStep(action, next) === undefined) void run(next)
    },
    [action, run],
  )
  const pickOnly = useCallback((filledNow: readonly Filled[], key: string, value: string, label: string) => advance(withAnswer(filledNow, key, value, 'auto', label)), [advance])
  const data = useStepData(step, filled, sources, pickOnly)

  const answer = (value: string | readonly string[]) => {
    if (step === undefined) return
    const label = typeof value === 'string' ? data?.choices?.find(choice => choice.value === value)?.label : undefined
    advance(withAnswer(filled, step.key, value, 'chosen', label))
  }
  const reopen = (key: string) => {
    setProblem(undefined)
    setFilled(backTo(filled, key))
  }
  const goBack = () => {
    setProblem(undefined)
    setFilled(back(filled))
  }

  const renderStep = () => {
    if (step === undefined && isRunning) return <p className="m-0 p-3 text-muted-foreground">Working…</p>
    // Everything was filled in by what was clicked: one press to go ahead.
    if (step === undefined) {
      return problem === undefined ? <PickList prompt={action.label} choices={[{ value: 'go', label: `${action.label} now` }]} onPick={() => void run(filled)} onBack={goBack} /> : null
    }
    if (data === undefined) return <p className="m-0 p-3 text-muted-foreground">Loading…</p>
    if (data.problem !== undefined) return <p className="m-0 p-3 text-danger">{data.problem}</p>
    const prompt = promptOf(step, answers, sources)
    const props: StepProps = { step, prompt, choices: data.choices, initial: data.initial, onAnswer: answer, onBack: goBack }
    switch (step.kind) {
      case 'text':
        return <TextStep key={step.key} {...props} placeholder={step.placeholder} isOptional={step.isOptional} />
      case 'longText':
        return <LongTextStep key={step.key} {...props} placeholder={step.placeholder} />
      case 'pick':
        return <PickList key={step.key} prompt={prompt} choices={data.choices ?? []} empty={step.empty} onPick={answer} onBack={goBack} />
      case 'pickMany':
        return <PickManyStep key={step.key} {...props} />
      case 'pickInOrder':
        return <PickInOrderStep key={step.key} {...props} />
      case 'confirm':
        return (
          <PickList
            key={step.key}
            prompt={action.label}
            choices={[
              { value: 'yes', label: step.yesLabel },
              { value: 'no', label: 'No, go back' },
            ]}
            onPick={picked => (picked === 'yes' ? answer('yes') : goBack())}
            onBack={goBack}
          />
        )
    }
  }

  const shown = stepsShown(action, filled)
  return (
    <div className="flex max-h-[calc(100vh-var(--space-8))] flex-col overflow-auto">
      <div className="border-b border-border px-3 py-2 font-medium">{action.label}</div>
      {problem !== undefined && (
        <div className="flex items-center gap-3 border-b border-border px-3 py-2 text-danger">
          <span className="flex-1">{problem}</span>
          {canGoBack(filled) && (
            <Button type="button" variant="ghost" size="sm" onClick={goBack}>
              Back
            </Button>
          )}
        </div>
      )}
      <ol className="m-0 flex list-none flex-col p-0">
        {shown.map((shownStep, index) => {
          const entry = filled.find(filledEntry => filledEntry.key === shownStep.key)
          const isCurrent = shownStep.key === step?.key
          return (
            <StepRow
              key={shownStep.key}
              number={index + 1}
              title={stepTitle(shownStep, answers, sources, entry !== undefined || isCurrent)}
              entry={entry}
              isCurrent={isCurrent}
              sources={sources}
              onReopen={() => reopen(shownStep.key)}
            >
              {renderStep()}
            </StepRow>
          )
        })}
      </ol>
      {step === undefined && renderStep()}
    </div>
  )
}
