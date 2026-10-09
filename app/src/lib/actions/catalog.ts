// Every action the app can take, the same ones and in the same order as the
// legion2 command: one per command and action word, each a list of steps.

import type { Command } from '@/generated/Command'
import type { Mission } from '@/generated/Mission'
import type { DeploymentSnapshot } from '@/lib/escalations'
import { askFor, askLegion } from '@/lib/legion'
import { NO_PIPELINE, pipelineLabel } from '@/lib/format'
import { COMMANDER } from '@/lib/roster'

import { listOf, textOf } from './flow'
import type { Action, Answers, Choice, Sources, Step } from './types'

// The deployment list's choice for starting a new one first.
export const NEW_DEPLOYMENT = '\u0000new'
const NO_MISSION = 'none'
const KEEP = 'keep'

const PERMISSION_MODES: readonly Choice[] = [
  { value: 'auto', label: 'auto', hint: 'Claude decides what’s safe; asks about the rest' },
  { value: 'acceptEdits', label: 'acceptEdits', hint: 'file edits go ahead; other tools ask' },
  { value: 'manual', label: 'manual', hint: 'every tool asks' },
  { value: 'bypassPermissions', label: 'bypassPermissions', hint: 'nothing asks' },
  { value: 'dontAsk', label: 'dontAsk', hint: 'refuses what would ask' },
  { value: 'plan', label: 'plan', hint: 'plans only, changes nothing' },
]
const MODELS: readonly Choice[] = [
  { value: 'default', label: 'default', hint: 'the session’s own default' },
  { value: 'opus', label: 'opus', hint: 'the most capable' },
  { value: 'sonnet', label: 'sonnet', hint: 'fast and capable' },
  { value: 'haiku', label: 'haiku', hint: 'quickest, for simple jobs' },
]
const SWITCHES: readonly Choice[] = [
  { value: 'off', label: 'off', hint: 'never' },
  { value: 'ask', label: 'ask', hint: 'ask me each time, as a question' },
  { value: 'free', label: 'free', hint: 'always, without asking' },
]
const KEYS: readonly Choice[] = ['enter', 'esc', 'up', 'down', 'tab'].map(key => ({ value: key, label: key }))
const GOING: readonly Mission['status'][] = ['started', 'handed_off', 'blocked']

const send = async (command: Command, done: string): Promise<string> => {
  const reply = await askLegion(command)
  return reply.type === 'text' ? reply.text : done
}

const firstLine = (text: string) => text.split('\n').find(line => line.trim() !== '' && !line.startsWith('#'))?.trim() ?? ''

const snapshotOf = (sources: Sources, deploymentId: string): DeploymentSnapshot | undefined =>
  sources.snapshots.find(snapshot => snapshot.deployment.id === deploymentId)

const folderOfDeployment = (sources: Sources, answers: Answers) => snapshotOf(sources, textOf(answers.deployment))?.deployment.folder ?? ''

// The folder an action works in: chosen, or the deployment's.
const folderPath = (answers: Answers, sources: Sources) => textOf(answers.folder) || folderOfDeployment(sources, answers)

// The steps that pick what an action is about.
const folderStep = (when?: Step['when']): Step => ({
  key: 'folder',
  kind: 'pick',
  prompt: 'Which folder?',
  when,
  empty: 'No folders yet: add one first.',
  choices: (_, sources) => sources.folders.map(folder => ({ value: folder.path, label: folder.name, hint: folder.path })),
})

const openDeploymentChoices = (answers: Answers, sources: Sources, wanted: (snapshot: DeploymentSnapshot) => boolean = () => true): Choice[] =>
  sources.snapshots
    .filter(snapshot => snapshot.deployment.closed_ms === null && wanted(snapshot))
    .filter(snapshot => textOf(answers.folder) === '' || snapshot.deployment.folder === textOf(answers.folder))
    .map(snapshot => ({ value: snapshot.deployment.id, label: snapshot.deployment.name, hint: `${pipelineLabel(snapshot.deployment.pipeline)} · ${snapshot.deployment.folder.split('/').at(-1) ?? ''}` }))

const deploymentStep = (wanted?: (snapshot: DeploymentSnapshot) => boolean): Step => ({
  key: 'deployment',
  kind: 'pick',
  prompt: 'Which deployment?',
  empty: 'No deployment is running: start one first.',
  choices: (answers, sources) => openDeploymentChoices(answers, sources, wanted),
})

const closedDeploymentStep: Step = {
  key: 'deployment',
  kind: 'pick',
  prompt: 'Which closed deployment?',
  empty: 'No closed deployments here.',
  choices: (answers, sources) =>
    sources.snapshots
      .filter(snapshot => snapshot.deployment.closed_ms !== null)
      .filter(snapshot => textOf(answers.folder) === '' || snapshot.deployment.folder === textOf(answers.folder))
      .map(snapshot => ({ value: snapshot.deployment.id, label: snapshot.deployment.name, hint: pipelineLabel(snapshot.deployment.pipeline) })),
}

const missionStep = (empty: string, wanted: (mission: Mission) => boolean): Step => ({
  key: 'mission',
  kind: 'pick',
  prompt: 'Which mission?',
  empty,
  choices: (answers, sources) =>
    (snapshotOf(sources, textOf(answers.deployment))?.missions ?? [])
      .filter(wanted)
      .map(mission => ({ value: String(mission.number), label: `#${mission.number} ${mission.title}`, hint: mission.status.replace('_', ' ') })),
})

const positionStep = (prompt: string): Step => ({
  key: 'position',
  kind: 'pick',
  prompt,
  empty: 'Nobody is running in this deployment.',
  choices: (answers, sources) =>
    (snapshotOf(sources, textOf(answers.deployment))?.sessions ?? []).map(session => ({
      value: session.position,
      label: session.position,
      hint: `${session.activity}${session.mission === null ? '' : ` on #${session.mission}`}`,
    })),
})

const operatorChoices = async (answers: Answers, sources: Sources): Promise<Choice[]> => {
  const detail = await sources.folderDetail(folderPath(answers, sources))
  return detail.operators.filter(operator => operator.problem === null).map(operator => ({ value: operator.name, label: operator.name, hint: firstLine(operator.definition) }))
}

const operatorStep: Step = { key: 'operator', kind: 'pick', prompt: 'Which operator?', empty: 'No operators yet: add one first.', choices: operatorChoices }

const filePipelineStep: Step = {
  key: 'pipeline',
  kind: 'pick',
  prompt: 'Which pipeline?',
  empty: 'No pipeline files yet: add one first.',
  choices: async (answers, sources) =>
    (await sources.folderDetail(folderPath(answers, sources))).pipelines
      .filter(pipeline => pipeline.file_text !== null)
      .map(pipeline => ({ value: pipeline.name, label: pipeline.name, hint: pipeline.operators.join(', ') })),
}

const confirmStep = (yesLabel: string): Step => ({ key: 'confirm', kind: 'confirm', prompt: 'Are you sure?', yesLabel })

const longText = (key: string, prompt: string | ((answers: Answers, sources: Sources) => string), placeholder: string): Step => ({ key, kind: 'longText', prompt, placeholder })

// Starting a deployment, also used by "Add a mission" when none is running.
const isNew = (answers: Answers) => textOf(answers.deployment) === NEW_DEPLOYMENT
const deploymentSetupSteps = (when: (answers: Answers) => boolean = () => true): Step[] => [
  folderStep(answers => when(answers) && textOf(answers.folder) === ''),
  {
    key: 'route',
    kind: 'pick',
    prompt: 'How should work move between operators?',
    when,
    empty: '',
    choices: async (answers, sources) => [
      { value: NO_PIPELINE, label: 'No pipeline', hint: 'after each step the commander picks who goes next' },
      ...(await sources.folderDetail(folderPath(answers, sources))).pipelines
        .filter(pipeline => pipeline.problem === null && pipeline.name !== NO_PIPELINE)
        .map(pipeline => ({ value: pipeline.name, label: `The ${pipeline.name} pipeline`, hint: pipeline.first === null ? `${pipeline.operators.join(', ')} through the commander` : `starts with ${pipeline.first}` })),
    ],
  },
  { key: 'team', kind: 'pickMany', prompt: 'Who’s on the team?', when: answers => when(answers) && answers.route === NO_PIPELINE, empty: 'No operators yet.', choices: operatorChoices },
  { key: 'name', kind: 'text', prompt: 'Name the deployment', when, initial: answers => (answers.route === NO_PIPELINE ? 'team' : textOf(answers.route)) },
]

// Starts the deployment the answers describe; with no pipeline, a team smaller than every
// operator is saved as a pipeline named after the deployment.
const startDeployment = async (answers: Answers, sources: Sources): Promise<string> => {
  const folder = folderPath(answers, sources)
  const detail = await sources.folderDetail(folder)
  const team = listOf(answers.team)
  const isEveryone = answers.route !== NO_PIPELINE || team.length === detail.operators.filter(operator => operator.problem === null).length
  const name = textOf(answers.name).trim()
  const teamName = `${name || 'team'}-team`
  if (!isEveryone) await askLegion({ type: 'pipeline_add', folder, name: teamName, operators: [...team], in_order: false })
  const { deployment } = await askFor('deployment', { type: 'deployment_start', folder, pipeline: isEveryone ? textOf(answers.route) : teamName, name: name || null })
  sources.goTo.deploymentPart(deployment.id, 'operators')
  return deployment.id
}

const post = (deployment: string, kind: 'paused' | 'resumed' | 'answer' | 'message', text: string, extra: { mission?: number; to?: string; answers?: number } = {}) =>
  askLegion({ type: 'post', deployment, entry: { kind, mission: extra.mission ?? null, to: extra.to ?? null, text, answers: extra.answers ?? null } })

export const ACTIONS: readonly Action[] = [
  // Folders: `legion2 setup`.
  {
    id: 'folder.add',
    group: 'Folder',
    label: 'Add a folder',
    subjects: ['app'],
    steps: [{ key: 'path', kind: 'text', prompt: 'The folder’s full path', placeholder: '/Users/you/code/app' }],
    run: async (answers, sources) => {
      const { folder, created_setup } = await askFor('folder', { type: 'folder_add', path: textOf(answers.path).trim() })
      sources.goTo.folder(folder.path)
      return created_setup ? `Set up .legion2/ in ${folder.name}; commit it so others get it.` : `Added ${folder.name}.`
    },
  },
  {
    id: 'folder.settings',
    group: 'Folder',
    label: 'Change folder settings',
    subjects: ['folder'],
    steps: [
      folderStep(),
      { key: 'check', kind: 'text', prompt: 'The command that checks a mission’s work (blank for none)', isOptional: true, initial: async (answers, sources) => (await sources.folderDetail(folderPath(answers, sources))).check ?? '' },
      { key: 'mode', kind: 'pick', prompt: 'Which permission mode should sessions start in?', empty: '', choices: () => [{ value: KEEP, label: 'Leave it' }, ...PERMISSION_MODES] },
    ],
    run: (answers, sources) =>
      send({ type: 'settings_set', folder: folderPath(answers, sources), check: textOf(answers.check), permission_mode: answers.mode === KEEP ? null : textOf(answers.mode) }, 'Saved.'),
  },
  {
    id: 'folder.remove',
    group: 'Folder',
    label: 'Remove a folder from Legion',
    subjects: ['folder'],
    isDestructive: true,
    steps: [folderStep(), confirmStep('Remove it; its setup, missions and log stay on disk')],
    run: (answers, sources) => send({ type: 'folder_remove', folder: folderPath(answers, sources) }, 'Removed.'),
  },
  // Operators.
  {
    id: 'operator.add',
    group: 'Operator',
    label: 'Add an operator',
    subjects: ['folder', 'operator'],
    steps: [
      folderStep(),
      { key: 'name', kind: 'text', prompt: 'Name the operator', placeholder: 'poem-writer' },
      longText('definition', answers => `What does ${textOf(answers.name)} do?`, 'You write one haiku per mission into poems/<slug>.txt.'),
      { key: 'copies', kind: 'text', prompt: 'How many copies may run at once?', initial: () => '1' },
      { key: 'model', kind: 'pick', prompt: 'Which model?', empty: '', choices: () => [...MODELS] },
    ],
    run: async (answers, sources) => {
      const folder = folderPath(answers, sources)
      const name = textOf(answers.name).trim()
      const limit = Number.parseInt(textOf(answers.copies), 10)
      if (!Number.isInteger(limit) || limit < 1) throw new Error(`${textOf(answers.copies)} isn't a number of copies`)
      await askLegion({ type: 'operator_add', folder, name, definition: textOf(answers.definition), limit: limit === 1 ? null : limit })
      if (answers.model !== 'default') await askLegion({ type: 'operator_set', folder, name, limit: null, model: textOf(answers.model), permission_mode: null })
      return `Added ${name}; deployments with no pipeline have it from now on.`
    },
  },
  {
    id: 'operator.define',
    group: 'Operator',
    label: 'Edit what an operator does',
    subjects: ['operator'],
    steps: [
      folderStep(),
      operatorStep,
      {
        key: 'definition',
        kind: 'longText',
        prompt: answers => `What ${textOf(answers.operator)} does`,
        initial: async (answers, sources) => (await sources.folderDetail(folderPath(answers, sources))).operators.find(operator => operator.name === answers.operator)?.definition ?? '',
      },
    ],
    run: (answers, sources) => send({ type: 'operator_define', folder: folderPath(answers, sources), name: textOf(answers.operator), definition: textOf(answers.definition) }, 'Saved.'),
  },
  {
    id: 'operator.settings',
    group: 'Operator',
    label: 'Change an operator’s copies, model or mode',
    subjects: ['operator'],
    steps: [
      folderStep(),
      operatorStep,
      {
        key: 'copies',
        kind: 'text',
        prompt: 'How many copies may run at once?',
        initial: async (answers, sources) => String((await sources.folderDetail(folderPath(answers, sources))).operators.find(operator => operator.name === answers.operator)?.copy_limit ?? 1),
      },
      { key: 'model', kind: 'pick', prompt: 'Which model?', empty: '', choices: () => [...MODELS] },
      { key: 'mode', kind: 'pick', prompt: 'Which permission mode should it start in?', empty: '', choices: () => [{ value: KEEP, label: 'Leave it' }, ...PERMISSION_MODES] },
    ],
    run: (answers, sources) => {
      const limit = Number.parseInt(textOf(answers.copies), 10)
      if (!Number.isInteger(limit) || limit < 1) return Promise.reject(new Error(`${textOf(answers.copies)} isn't a number of copies`))
      return send(
        { type: 'operator_set', folder: folderPath(answers, sources), name: textOf(answers.operator), limit, model: textOf(answers.model), permission_mode: answers.mode === KEEP ? null : textOf(answers.mode) },
        'Saved.',
      )
    },
  },
  {
    id: 'operator.remove',
    group: 'Operator',
    label: 'Remove an operator',
    subjects: ['operator'],
    isDestructive: true,
    steps: [folderStep(), operatorStep, confirmStep('Remove it and its definition')],
    run: (answers, sources) => send({ type: 'operator_remove', folder: folderPath(answers, sources), name: textOf(answers.operator) }, 'Removed.'),
  },
  // Pipelines.
  {
    id: 'pipeline.add',
    group: 'Pipeline',
    label: 'Add a pipeline',
    subjects: ['folder', 'pipeline'],
    steps: [
      folderStep(),
      { key: 'name', kind: 'text', prompt: 'Name the pipeline', placeholder: 'poems' },
      {
        key: 'route',
        kind: 'pick',
        prompt: 'How should work move between its operators?',
        empty: '',
        choices: () => [
          { value: 'commander', label: 'No set order', hint: 'after each step the commander picks who goes next' },
          { value: 'in-order', label: 'In order', hint: 'each operator passes to the next' },
        ],
      },
      { key: 'team', kind: 'pickMany', prompt: 'Who’s on it?', when: answers => answers.route === 'commander', empty: 'No operators yet.', choices: operatorChoices },
      { key: 'steps', kind: 'pickInOrder', prompt: 'Pick each step in order', when: answers => answers.route === 'in-order', empty: 'No operators yet.', choices: operatorChoices },
    ],
    run: (answers, sources) => {
      const isInOrder = answers.route === 'in-order'
      const operators = [...listOf(isInOrder ? answers.steps : answers.team)]
      return send({ type: 'pipeline_add', folder: folderPath(answers, sources), name: textOf(answers.name).trim(), operators, in_order: isInOrder }, 'Added.')
    },
  },
  {
    id: 'pipeline.edit',
    group: 'Pipeline',
    label: 'Edit a pipeline’s file',
    subjects: ['pipeline'],
    steps: [
      folderStep(),
      filePipelineStep,
      {
        key: 'text',
        kind: 'longText',
        prompt: answers => `The ${textOf(answers.pipeline)} pipeline`,
        initial: async (answers, sources) => (await sources.folderDetail(folderPath(answers, sources))).pipelines.find(pipeline => pipeline.name === answers.pipeline)?.file_text ?? '',
      },
    ],
    run: (answers, sources) => send({ type: 'pipeline_write', folder: folderPath(answers, sources), name: textOf(answers.pipeline), text: textOf(answers.text) }, 'Saved.'),
  },
  {
    id: 'pipeline.remove',
    group: 'Pipeline',
    label: 'Remove a pipeline',
    subjects: ['pipeline'],
    isDestructive: true,
    steps: [folderStep(), filePipelineStep, confirmStep('Remove it')],
    run: (answers, sources) => send({ type: 'pipeline_remove', folder: folderPath(answers, sources), name: textOf(answers.pipeline) }, 'Removed.'),
  },
  // Deployments: `legion2 deploy`.
  {
    id: 'deployment.start',
    group: 'Deploy',
    label: 'Start a deployment',
    subjects: ['folder', 'deployment'],
    steps: deploymentSetupSteps(),
    run: async (answers, sources) => {
      await startDeployment(answers, sources)
      return 'Started; its commander is starting.'
    },
  },
  {
    id: 'deployment.rename',
    group: 'Deploy',
    label: 'Rename a deployment',
    subjects: ['deployment'],
    steps: [deploymentStep(), { key: 'name', kind: 'text', prompt: 'Its new name', initial: (answers, sources) => snapshotOf(sources, textOf(answers.deployment))?.deployment.name ?? '' }],
    run: (answers) => send({ type: 'deployment_rename', deployment: textOf(answers.deployment), name: textOf(answers.name).trim() }, 'Renamed.'),
  },
  {
    id: 'deployment.repipe',
    group: 'Deploy',
    label: 'Change a deployment’s pipeline',
    subjects: ['deployment'],
    steps: [
      deploymentStep(),
      {
        key: 'pipeline',
        kind: 'pick',
        prompt: 'Move it to which pipeline?',
        empty: 'No other pipeline can run here.',
        choices: async (answers, sources) => {
          const current = snapshotOf(sources, textOf(answers.deployment))?.deployment.pipeline
          return (await sources.folderDetail(folderPath(answers, sources))).pipelines
            .filter(pipeline => pipeline.problem === null && pipeline.name !== current)
            .map(pipeline => ({ value: pipeline.name, label: pipelineLabel(pipeline.name), hint: pipeline.operators.join(', ') }))
        },
      },
    ],
    run: (answers) => send({ type: 'deployment_repipe', deployment: textOf(answers.deployment), pipeline: textOf(answers.pipeline) }, 'Moved; its commander hears the new team shortly.'),
  },
  {
    id: 'deployment.stop',
    group: 'Deploy',
    label: 'Stop every session',
    subjects: ['deployment'],
    steps: [deploymentStep(snapshot => snapshot.sessions.length > 0), confirmStep('Stop them; it stays open to start again')],
    run: async (answers, sources) => {
      const deployment = textOf(answers.deployment)
      const sessions = snapshotOf(sources, deployment)?.sessions ?? []
      await Promise.all(sessions.map(session => askLegion({ type: 'session_stop', deployment, position: session.position })))
      return 'Stopped.'
    },
  },
  {
    id: 'deployment.restart',
    group: 'Deploy',
    label: 'Start a stopped deployment again',
    subjects: ['deployment'],
    steps: [deploymentStep(snapshot => snapshot.sessions.length === 0)],
    run: (answers) => send({ type: 'session_start', deployment: textOf(answers.deployment), operator: COMMANDER, mission: null, part: null }, 'Its commander is starting.'),
  },
  {
    id: 'deployment.close',
    group: 'Deploy',
    label: 'Close a deployment',
    subjects: ['deployment'],
    isDestructive: true,
    steps: [deploymentStep(), confirmStep('Close it; every session ends for good')],
    run: (answers) => send({ type: 'deployment_close', deployment: textOf(answers.deployment) }, 'Closed.'),
  },
  {
    id: 'deployment.reopen',
    group: 'Deploy',
    label: 'Reopen a closed deployment',
    subjects: ['closedDeployment', 'folder'],
    steps: [closedDeploymentStep],
    run: async (answers, sources) => {
      const { deployment } = await askFor('deployment', { type: 'deployment_reopen', deployment: textOf(answers.deployment) })
      sources.goTo.deploymentPart(deployment.id, 'operators')
      return `Reopened ${deployment.name}; its commander is starting.`
    },
  },
  {
    id: 'deployment.delete',
    group: 'Deploy',
    label: 'Delete a closed deployment',
    subjects: ['closedDeployment'],
    isDestructive: true,
    steps: [closedDeploymentStep, confirmStep('Delete it for good: its log, missions and checkouts go; its branches stay in git')],
    run: (answers) => send({ type: 'deployment_delete', deployment: textOf(answers.deployment) }, 'Deleted.'),
  },
  {
    id: 'deployment.switches',
    group: 'Channel',
    label: 'Set a deployment’s channel switches',
    subjects: ['deployment', 'channel'],
    steps: [
      deploymentStep(),
      { key: 'send', kind: 'pick', prompt: 'Can its commander message other teams?', empty: '', choices: () => [...SWITCHES] },
      { key: 'receive', kind: 'pick', prompt: 'Do other teams’ messages reach it?', empty: '', choices: () => [...SWITCHES] },
    ],
    run: async (answers) => {
      const isSwitch = (value: string): value is 'off' | 'ask' | 'free' => SWITCHES.some(choice => choice.value === value)
      const [sendSwitch, receiveSwitch] = [textOf(answers.send), textOf(answers.receive)]
      if (!isSwitch(sendSwitch) || !isSwitch(receiveSwitch)) throw new Error('pick off, ask or free')
      await askLegion({ type: 'channel_switch', deployment: textOf(answers.deployment), send: sendSwitch, receive: receiveSwitch })
      return 'Saved.'
    },
  },
  // Missions.
  {
    id: 'mission.add',
    group: 'Mission',
    label: 'Add a mission',
    subjects: ['folder', 'deployment', 'mission'],
    steps: [
      {
        key: 'deployment',
        kind: 'pick',
        prompt: 'Which deployment should do it?',
        empty: '',
        choices: (answers, sources) => [...openDeploymentChoices(answers, sources), { value: NEW_DEPLOYMENT, label: 'Start a new deployment for it', hint: 'set one up first' }],
      },
      ...deploymentSetupSteps(isNew),
      { key: 'title', kind: 'text', prompt: 'Mission title', placeholder: 'Autumn rain haiku' },
      longText('body', 'What should it get done?', 'Details, links, and what done looks like.'),
    ],
    run: async (answers, sources) => {
      const deployment = isNew(answers) ? await startDeployment(answers, sources) : textOf(answers.deployment)
      const reply = await askFor('missions', { type: 'mission_add', deployment, title: textOf(answers.title).trim(), body: textOf(answers.body) })
      const added = reply.missions[0]
      if (added !== undefined) sources.goTo.mission(deployment, added.number)
      return added === undefined ? 'Added.' : `Added mission #${added.number}.`
    },
  },
  {
    id: 'mission.pause',
    group: 'Mission',
    label: 'Pause a mission',
    subjects: ['mission'],
    steps: [deploymentStep(), missionStep('No mission is in progress.', mission => GOING.includes(mission.status)), longText('note', 'Why pause it?', 'Waiting on the design review.')],
    run: async (answers) => {
      await post(textOf(answers.deployment), 'paused', textOf(answers.note), { mission: Number(answers.mission) })
      return 'Paused; whoever holds it stops at the next good point.'
    },
  },
  {
    id: 'mission.resume',
    group: 'Mission',
    label: 'Resume a paused mission',
    subjects: ['mission'],
    steps: [deploymentStep(), missionStep('No mission is paused.', mission => mission.status === 'paused'), longText('note', 'Anything to tell whoever picks it up?', 'The review is done: go ahead.')],
    run: async (answers) => {
      await post(textOf(answers.deployment), 'resumed', textOf(answers.note), { mission: Number(answers.mission) })
      return 'Resumed.'
    },
  },
  {
    id: 'mission.finish',
    group: 'Mission',
    label: 'Finish a done mission',
    subjects: ['mission'],
    steps: [deploymentStep(), missionStep('No mission is done yet.', mission => mission.status === 'done')],
    run: (answers) => send({ type: 'mission_finish', deployment: textOf(answers.deployment), mission: Number(answers.mission) }, 'Finished.'),
  },
  // Talking to the team: `legion2 answer` and `legion2 send`.
  {
    id: 'question.answer',
    group: 'Answer',
    label: 'Answer a question',
    subjects: ['question', 'deployment'],
    steps: [
      deploymentStep(snapshot => snapshot.openQuestions.length > 0),
      {
        key: 'question',
        kind: 'pick',
        prompt: 'Which question?',
        empty: 'No questions are waiting on you.',
        choices: (answers, sources) =>
          (snapshotOf(sources, textOf(answers.deployment))?.openQuestions ?? []).map(entry => ({ value: String(entry.id), label: `#${entry.id} ${firstLine(entry.text) || entry.text}`, hint: `from ${entry.from}` })),
      },
      longText(
        'answer',
        (answers, sources) => {
          const asked = snapshotOf(sources, textOf(answers.deployment))?.openQuestions.find(entry => String(entry.id) === answers.question)
          return asked === undefined ? 'Your answer' : `${asked.from} asks: ${asked.text}`
        },
        'yes',
      ),
    ],
    run: async (answers) => {
      await post(textOf(answers.deployment), 'answer', textOf(answers.answer), { answers: Number(answers.question) })
      return 'Answered.'
    },
  },
  {
    id: 'session.send',
    group: 'Send',
    label: 'Send someone a message',
    subjects: ['session', 'deployment'],
    steps: [deploymentStep(snapshot => snapshot.sessions.length > 0), positionStep('Who to?'), longText('message', answers => `Message for ${textOf(answers.position)}`, 'Please check the failing test first.')],
    run: async (answers) => {
      await post(textOf(answers.deployment), 'message', textOf(answers.message), { to: textOf(answers.position) })
      return 'Sent.'
    },
  },
  // Sessions.
  {
    id: 'session.start',
    group: 'Session',
    label: 'Start a session',
    subjects: ['session', 'deployment'],
    steps: [
      deploymentStep(),
      {
        key: 'operator',
        kind: 'pick',
        prompt: 'Start whom?',
        empty: '',
        choices: (answers, sources) => [
          { value: COMMANDER, label: COMMANDER, hint: 'leads the deployment' },
          ...(snapshotOf(sources, textOf(answers.deployment))?.pipelineOperators ?? []).map(operator => ({ value: operator, label: operator })),
        ],
      },
      {
        key: 'mission',
        kind: 'pick',
        prompt: 'On which mission?',
        when: answers => answers.operator !== COMMANDER,
        empty: '',
        choices: (answers, sources) => [
          { value: NO_MISSION, label: 'No mission', hint: 'it waits for one' },
          ...(snapshotOf(sources, textOf(answers.deployment))?.missions ?? []).filter(mission => mission.status !== 'done').map(mission => ({ value: String(mission.number), label: `#${mission.number} ${mission.title}` })),
        ],
      },
    ],
    run: (answers) => {
      const mission = answers.mission === undefined || answers.mission === NO_MISSION ? null : Number(answers.mission)
      return send({ type: 'session_start', deployment: textOf(answers.deployment), operator: textOf(answers.operator), mission, part: null }, 'Starting.')
    },
  },
  {
    id: 'session.stop',
    group: 'Session',
    label: 'Stop a session',
    subjects: ['session'],
    isDestructive: true,
    steps: [deploymentStep(snapshot => snapshot.sessions.length > 0), positionStep('Stop which session?'), confirmStep('Stop it')],
    run: (answers) => send({ type: 'session_stop', deployment: textOf(answers.deployment), position: textOf(answers.position) }, 'Stopped.'),
  },
  {
    id: 'session.key',
    group: 'Session',
    label: 'Press a key in a session',
    subjects: ['session'],
    steps: [deploymentStep(snapshot => snapshot.sessions.length > 0), positionStep('Which session?'), { key: 'key', kind: 'pick', prompt: 'Press which key?', empty: '', choices: () => [...KEYS] }],
    run: (answers) => send({ type: 'key', deployment: textOf(answers.deployment), position: textOf(answers.position), key: textOf(answers.key) }, 'Pressed.'),
  },
  // Channels.
  {
    id: 'channel.open',
    group: 'Channel',
    label: 'Host a channel',
    subjects: ['channel', 'app'],
    steps: [
      { key: 'port', kind: 'text', prompt: 'Which port?', initial: () => '4620' },
      { key: 'key', kind: 'text', prompt: 'The key subscribers give (blank makes one)', isOptional: true },
    ],
    run: (answers) => {
      const port = Number.parseInt(textOf(answers.port), 10)
      if (!Number.isInteger(port)) return Promise.reject(new Error(`${textOf(answers.port)} isn't a port`))
      return send({ type: 'channel_open', port, key: textOf(answers.key).trim() || null }, 'Hosting.')
    },
  },
  {
    id: 'channel.close',
    group: 'Channel',
    label: 'Stop hosting the channel',
    subjects: ['channel'],
    isDestructive: true,
    steps: [confirmStep('Stop hosting; its subscribers are cut off')],
    run: () => send({ type: 'channel_close' }, 'Stopped hosting.'),
  },
  {
    id: 'channel.subscribe',
    group: 'Channel',
    label: 'Subscribe to another Legion’s channel',
    subjects: ['channel', 'app'],
    steps: [
      { key: 'address', kind: 'text', prompt: 'The channel’s address', placeholder: 'spark.tailb50373.ts.net:4620' },
      { key: 'key', kind: 'text', prompt: 'Its key' },
    ],
    run: (answers) => send({ type: 'channel_subscribe', address: textOf(answers.address).trim(), key: textOf(answers.key).trim() }, 'Subscribed.'),
  },
  {
    id: 'channel.unsubscribe',
    group: 'Channel',
    label: 'Unsubscribe from a channel',
    subjects: ['channel'],
    steps: [
      {
        key: 'address',
        kind: 'pick',
        prompt: 'Unsubscribe from which?',
        empty: 'Not subscribed to any channel.',
        choices: (_, sources) => (sources.channels?.subscriptions ?? []).map(subscription => ({ value: subscription.address, label: subscription.address, hint: subscription.is_up ? 'up' : 'down' })),
      },
    ],
    run: (answers) => send({ type: 'channel_unsubscribe', address: textOf(answers.address) }, 'Unsubscribed.'),
  },
]

export const actionById = (id: string): Action | undefined => ACTIONS.find(action => action.id === id)

export const actionsFor = (subject: Action['subjects'][number]): readonly Action[] => ACTIONS.filter(action => action.subjects.includes(subject))
