import { useEffect, useState } from 'react'
import { engineInfo, loadEngine } from './engine/index.ts'
import type { EngineInfo } from './engine/types/EngineInfo.ts'

type EngineStatus =
  | { kind: 'loading' }
  | { kind: 'ready'; info: EngineInfo }
  | { kind: 'failed'; message: string }

function App() {
  const [status, setStatus] = useState<EngineStatus>({ kind: 'loading' })

  useEffect(() => {
    let cancelled = false
    loadEngine()
      .then(() => {
        if (!cancelled) setStatus({ kind: 'ready', info: engineInfo() })
      })
      .catch((error: unknown) => {
        if (!cancelled) setStatus({ kind: 'failed', message: String(error) })
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <main>
      <h1>Magic Roguelike</h1>
      {status.kind === 'loading' && <p>Loading the rules engine…</p>}
      {status.kind === 'ready' && (
        <p data-testid="engine-info">
          Rules engine loaded from Rust: {status.info.name} {status.info.version}
        </p>
      )}
      {status.kind === 'failed' && (
        <p role="alert">The rules engine failed to load: {status.message}</p>
      )}
    </main>
  )
}

export default App
