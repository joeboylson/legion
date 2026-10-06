// When drawing the app throws, show what went wrong instead of a white page.

import { Component, type ReactNode } from 'react'

type CrashScreenState = { error?: Error }

// React catches drawing errors only in a class component.
export class CrashScreen extends Component<{ children: ReactNode }, CrashScreenState> {
  state: CrashScreenState = {}

  static getDerivedStateFromError(error: Error): CrashScreenState {
    return { error }
  }

  render() {
    const { error } = this.state
    if (error === undefined) return this.props.children
    return (
      <main className="flex flex-col gap-3 p-5">
        <span className="label text-danger">The app crashed</span>
        <p className="m-0 font-medium">{error.message}</p>
        <pre className="m-0 overflow-auto font-mono text-label whitespace-pre-wrap text-muted-foreground">{error.stack}</pre>
      </main>
    )
  }
}
