import { useEffect, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import example from '../../crates/core/spec/example.json'
import { Dependencies } from './dependencies'
import { Button } from './flint'
import { Overview } from './overview'
import { type Loaded, describe, load } from './report'

const SCREENS = [
  { id: 'overview', label: 'Overview' },
  { id: 'imports', label: 'Module imports' },
  { id: 'dependencies', label: 'External dependencies' },
] as const
type Screen = (typeof SCREENS)[number]

const STORE = 'rust-view.reports'
const EMPTY: Loaded = { crates: [], errors: [] }

const current = (): Screen => SCREENS.find(s => `#${s.id}` === location.hash) ?? SCREENS[0]

/** The reports from the last visit; storage may be missing, full, or stale. */
const restore = (): Loaded => {
  try {
    const saved = localStorage.getItem(STORE)
    return saved ? { ...load(JSON.parse(saved)), errors: [] } : EMPTY
  } catch {
    return EMPTY
  }
}

function App() {
  const [screen, setScreen] = useState(current)
  const [loaded, setLoaded] = useState(restore)
  const picker = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const onHash = () => setScreen(current())
    addEventListener('hashchange', onHash)
    return () => removeEventListener('hashchange', onHash)
  }, [])

  const open = (inputs: { name: string; text: string | null }[]) => {
    const next = load(inputs)
    // Keep what is on screen when nothing in the new files could be read.
    setLoaded(next.crates.length ? next : { ...loaded, errors: next.errors })
    if (!next.crates.length) return
    try {
      localStorage.setItem(STORE, JSON.stringify(inputs))
    } catch {
      // Not saved: the reports stay loaded until the page reloads.
    }
  }
  const openFiles = async (files: FileList | null) => {
    if (!files?.length) return
    open(await Promise.all([...files].map(async f => ({ name: f.name, text: await f.text().catch(() => null) }))))
  }

  const { crates, errors } = loaded
  return (
    <div
      className="shell"
      onDragOver={e => e.preventDefault()}
      onDrop={e => {
        e.preventDefault()
        openFiles(e.dataTransfer.files)
      }}
    >
      <nav className="side" aria-label="Screens">
        <div className="side-name heading">rust-view</div>
        {SCREENS.map(s => (
          <a key={s.id} className="side-item" href={`#${s.id}`} aria-current={s === screen ? 'page' : undefined}>
            {s.label}
          </a>
        ))}
        <div className="side-foot">
          <span className="label">{crates.length === 1 ? '1 crate loaded' : `${crates.length} crates loaded`}</span>
          <Button size="sm" onClick={() => picker.current?.click()}>Open reports</Button>
          <input
            ref={picker}
            type="file"
            accept=".json,.jsonl,application/json"
            multiple
            hidden
            onChange={e => {
              openFiles(e.target.files)
              e.target.value = ''
            }}
          />
        </div>
      </nav>
      <main className={screen.id === 'dependencies' ? 'main fill' : 'main'}>
        <h1 className="title">{screen.label}</h1>
        {errors.length > 0 && (
          <ul className="errors small" role="alert">
            {errors.map((e, i) => <li key={i}>{describe(e)}</li>)}
          </ul>
        )}
        {crates.length === 0 ? (
          <div className="empty">
            <p className="body">
              No report is loaded. Run <span className="code">rust-view -o reports</span> in a cargo project, then open
              the files it writes or drop them here.
            </p>
            <div className="row">
              <Button variant="primary" onClick={() => picker.current?.click()}>Open reports</Button>
              <Button variant="ghost" onClick={() => open([{ name: 'example.json', text: JSON.stringify(example) }])}>
                Load example
              </Button>
            </div>
          </div>
        ) : screen.id === 'overview' ? (
          <Overview crates={crates} />
        ) : screen.id === 'dependencies' ? (
          <Dependencies crates={crates} />
        ) : (
          <p className="body muted">This screen is not built yet.</p>
        )}
      </main>
    </div>
  )
}

const root = document.getElementById('root')
if (root) createRoot(root).render(<App />)
