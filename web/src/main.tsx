import { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import example from '../../crates/core/spec/example.json'
import { Dependencies } from './dependencies'
import { Button, Tag } from './flint'
import { Flow } from './flow'
import { Items } from './items'
import { Overview } from './overview'
import { type Loaded, VERSION, describe, load, merge } from './report'
import { type Entry, STORED, list, read } from './store'

const SCREENS = [
  { id: 'overview', label: 'Overview', main: 'main' },
  { id: 'items', label: 'Module items', main: 'main fill' },
  { id: 'flow', label: 'Type flow', main: 'main fill bleed' },
  { id: 'dependencies', label: 'External dependencies', main: 'main fill bleed' },
] as const
type Screen = (typeof SCREENS)[number]

const PICK = 'rust-view.pick'
const EMPTY: Loaded = { crates: [], errors: [] }

const current = (): Screen => SCREENS.find(s => `#${s.id}` === location.hash) ?? SCREENS[0]

/** The project to show on arrival: the one picked last, or the newest if the CLI has written a report since. */
const recall = (entries: Entry[]): Entry => {
  try {
    const { id, seen } = JSON.parse(localStorage.getItem(PICK) ?? '{}')
    if (seen === entries[0].time) return entries.find(e => e.id === id) ?? entries[0]
  } catch {
    // Storage is missing or stale: the newest it is.
  }
  return entries[0]
}

/** The store's projects, or what to do when there are none. */
function Projects(props: { entries: Entry[]; picked: string | null; onPick: (e: Entry) => void; onExample: () => void }) {
  if (props.entries.length === 0) {
    return (
      <div className="empty">
        <p className="body">
          {STORED ? (
            <>No reports yet. Run <span className="code">rust-view .</span> in a cargo project, then choose Open reports.</>
          ) : (
            <>
              This page has no report store beside it. Drop report files here, or run{' '}
              <span className="code">rust-view . --open</span> in a cargo project for the installed dashboard.
            </>
          )}
        </p>
        <div className="row">
          <Button variant="ghost" onClick={props.onExample}>Load example</Button>
        </div>
      </div>
    )
  }
  return (
    <ul className="picks projects">
      {props.entries.map(e => (
        <li key={e.id}>
          <button
            type="button"
            className="pick"
            aria-current={e.id === props.picked ? 'true' : undefined}
            onClick={() => props.onPick(e)}
          >
            <span className="row">
              <span className="code">{e.name}</span>
              {e.version !== VERSION && <Tag tone="warning">Re-run needed</Tag>}
            </span>
            <span className="small muted">{e.path}</span>
            <span className="small muted">
              {e.crates.length === 1 ? '1 crate' : `${e.crates.length} crates`}, {new Date(e.time).toLocaleString()}
            </span>
          </button>
        </li>
      ))}
    </ul>
  )
}

function App() {
  const [screen, setScreen] = useState(current)
  const [loaded, setLoaded] = useState(EMPTY)
  const [entries, setEntries] = useState<Entry[]>([])
  const [picked, setPicked] = useState<string | null>(null)
  const [listing, setListing] = useState(false)
  // Nothing is drawn until the store's first project has had its chance to load.
  const [ready, setReady] = useState(!STORED)

  /** Shows `next`, or keeps what is on screen when none of it could be read. */
  const show = (next: Loaded, id: string | null) => {
    setLoaded(prev => (next.crates.length ? next : { ...prev, errors: next.errors }))
    if (!next.crates.length) return false
    setPicked(id)
    setListing(false)
    return true
  }
  const pick = async (entry: Entry, all: Entry[]) => {
    const docs = await read(entry.id)
    if (!show(merge([{ name: docs ? entry.name : `reports/${entry.id}.js`, docs }]), entry.id)) return
    try {
      localStorage.setItem(PICK, JSON.stringify({ id: entry.id, seen: all[0].time }))
    } catch {
      // Not remembered: the next visit starts on the newest project.
    }
  }
  const refresh = async () => {
    const found = await list()
    setEntries(found)
    return found
  }
  const openFiles = async (files: FileList | null) => {
    if (!files?.length) return
    const inputs = [...files].map(async f => ({ name: f.name, text: await f.text().catch(() => null) }))
    show(load(await Promise.all(inputs)), null)
  }

  useEffect(() => {
    const onHash = () => setScreen(current())
    addEventListener('hashchange', onHash)
    refresh()
      .then(found => (found.length ? pick(recall(found), found) : undefined))
      .finally(() => setReady(true))
    return () => removeEventListener('hashchange', onHash)
  }, [])

  const { crates, errors } = loaded
  const choosing = listing || crates.length === 0
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
          <a
            key={s.id}
            className="side-item"
            href={`#${s.id}`}
            aria-current={s === screen && !choosing ? 'page' : undefined}
            onClick={() => setListing(false)}
          >
            {s.label}
          </a>
        ))}
        <div className="side-foot">
          <span className="small">{entries.find(e => e.id === picked)?.name}</span>
          <span className="label">{crates.length === 1 ? '1 crate loaded' : `${crates.length} crates loaded`}</span>
          <Button
            size="sm"
            onClick={() => {
              setListing(true)
              refresh()
            }}
          >
            Open reports
          </Button>
        </div>
      </nav>
      <main className={choosing ? 'main' : screen.main}>
        {ready && (
          <header className="top">
            <h1 className="title">{choosing ? 'Reports' : screen.label}</h1>
            {errors.length > 0 && (
              <ul className="errors small" role="alert">
                {errors.map((e, i) => <li key={i}>{describe(e)}</li>)}
              </ul>
            )}
          </header>
        )}
        {!ready ? null : choosing ? (
          <Projects
            entries={entries}
            picked={picked}
            onPick={e => pick(e, entries)}
            onExample={() => show(merge([{ name: 'example.json', docs: [example] }]), null)}
          />
        ) : screen.id === 'overview' ? (
          <Overview crates={crates} />
        ) : screen.id === 'dependencies' ? (
          <Dependencies crates={crates} />
        ) : screen.id === 'flow' ? (
          <Flow crates={crates} />
        ) : (
          <Items crates={crates} />
        )}
      </main>
    </div>
  )
}

const root = document.getElementById('root')
if (root) createRoot(root).render(<App />)
