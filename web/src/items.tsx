import { type CSSProperties, type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import { Canvas } from './canvas'
import { Button, Tag } from './flint'
import { type Crate, type Definition, type File, KINDS, type Kind } from './report'

/** Node height and least width, row pitch, and indent per tree level. */
const HEIGHT = 40
const WIDTH = 128
const PITCH = 48
const INDENT = 32
/** Clear space inside the drawing's edges, and between the widest node and the first arrow lane. */
const MARGIN = 24
/** Distance between the lanes arrows run down, one per module they leave. */
const LANE = 16
/** Where arrows enter and leave a node, below its top edge. */
const IN = 12
const OUT = 28
/** The mono advance and side padding a node's width is estimated from. */
const ADVANCE = 7.8
const PAD = 16
/** Modules start expanded in the list up to this many items in total. */
const MAX_OPEN_ITEMS = 50

type Item = Definition & { id: string; file: string; root: string }
type Mod = {
  id: string
  module: string
  /** Last path segment; the crate's name for a target's root. */
  label: string
  /** A root's file, set when its crate has several targets. */
  note: string
  /** The file whose module this is; null for an inline module. */
  file: File | null
  children: Mod[]
  items: Item[]
}
type Row = { mod: Mod; x: number; y: number; width: number; parent: Row | null }
/** The focused item passing from one module to another that imports it; `via` a re-export if not from its definition. */
type Flow = { from: string; to: string; via: boolean }

/** Module paths repeat across a crate's targets, so a module is keyed by its target's root file too. */
const key = (root: string, module: string) => `${root} ${module}`

/** Each target's module tree. Modules that define, import, and re-export nothing (test code) are left out. */
const build = (crates: Crate[]): Mod[] => {
  const kept = (m: Mod): boolean => {
    m.children = m.children.filter(kept).sort((a, b) => a.label.localeCompare(b.label))
    return m.items.length > 0 || m.children.length > 0 || !!m.file && m.file.imports.length + m.file.reexports.length > 0
  }
  return crates.flatMap(crate => {
    const mods = new Map<string, Mod>()
    const targets: Mod[] = []
    const node = (root: string, module: string): Mod => {
      const id = key(root, module)
      const found = mods.get(id)
      if (found) return found
      const cut = module.lastIndexOf('::')
      const mod: Mod = { id, module, label: cut < 0 ? crate.name : module.slice(cut + 2), note: '', file: null, children: [], items: [] }
      mods.set(id, mod)
      if (cut < 0) targets.push(mod)
      else node(root, module.slice(0, cut)).children.push(mod)
      return mod
    }
    for (const file of crate.files) {
      node(file.root, file.module).file = file
      file.definitions.forEach((d, i) =>
        node(file.root, d.module).items.push({ ...d, id: `${file.path} ${i}`, file: file.path, root: file.root }),
      )
    }
    const shown = targets.filter(kept)
    const dir = `${crate.path}/`
    if (shown.length > 1) for (const t of shown) t.note = t.file?.path.replace(dir, '') ?? ''
    return shown
  })
}

/** Follows `item` from its module to every importer, and on from each module that re-exports it; also counts those. */
const follow = (item: Item, files: File[]): { flows: Flow[]; exporters: number } => {
  const flows: Flow[] = []
  const exporters = new Set<string>()
  const seen = new Set<string>()
  const todo = [{ module: item.module, file: item.file, name: item.name }]
  for (let at = todo.shift(); at; at = todo.shift()) {
    const here = `${at.file} ${at.module} ${at.name}`
    if (seen.has(here)) continue
    seen.add(here)
    const via = seen.size > 1
    const from = key(item.root, at.module)
    // A glob takes what its module can see: any public name, or a private one of a module around it.
    const globs = (f: File) => via || item.public || f.module.startsWith(`${at.module}::`)
    for (const f of files) {
      const takes = f.imports.some(i =>
        i.module === at.module && i.file === at.file && (i.names.includes(at.name) || (i.names.includes('*') && globs(f))),
      )
      if (takes) flows.push({ from, to: key(item.root, f.module), via })
      for (const r of f.reexports) {
        const hop = r.trace[0]
        if (!hop || hop.module !== at.module || hop.file !== at.file) continue
        const name = hop.name === at.name ? r.name : hop.name === '*' && globs(f) ? at.name : null
        if (name === null) continue
        exporters.add(f.path)
        todo.push({ module: f.module, file: f.path, name })
      }
    }
  }
  return { flows, exporters: exporters.size }
}

const count = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`

function Switch({ label, checked, onChange }: { label: string; checked: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="switch label">
      {label}
      <input type="checkbox" role="switch" checked={checked} onChange={e => onChange(e.target.checked)} />
    </label>
  )
}

export function Items({ crates }: { crates: Crate[] }) {
  /** The focused item's id, or the focused module's. */
  const [picked, setPicked] = useState<string | null>(null)
  const [reexports, setReexports] = useState(true)
  const [internal, setInternal] = useState(true)
  const [hidden, setHidden] = useState<ReadonlySet<Kind>>(new Set())
  const [toggled, setToggled] = useState<ReadonlySet<string>>(new Set())
  /** The focus as drawn in the tree, and as listed when it is a module. */
  const node = useRef<SVGGElement>(null)
  const line = useRef<HTMLButtonElement>(null)

  const roots = useMemo(() => build(crates), [crates])
  const rows = useMemo(() => {
    const out: Row[] = []
    const walk = (mod: Mod, depth: number, parent: Row | null) => {
      const text = mod.note ? `${mod.label} ${mod.note}` : mod.label
      const row = {
        mod,
        parent,
        x: MARGIN + depth * INDENT,
        y: MARGIN + out.length * PITCH,
        width: Math.max(WIDTH, 8 * Math.ceil((text.length * ADVANCE + 2 * PAD) / 8)),
      }
      out.push(row)
      mod.children.forEach(c => walk(c, depth + 1, row))
    }
    roots.forEach(r => walk(r, 0, null))
    return out
  }, [roots])
  const all = useMemo(() => rows.flatMap(r => r.mod.items), [rows])
  const item = all.find(i => i.id === picked)
  const spot = item ? undefined : rows.find(r => r.mod.id === picked)?.mod
  /** The module in focus: the focused item's, or the one picked in the tree. */
  const home = item ? key(item.root, item.module) : spot?.id ?? ''

  /** Every hop of the focused item, each pair of modules once. */
  const { flows: traced, exporters } = useMemo(() => {
    if (!item) return { flows: [], exporters: 0 }
    const { flows, exporters } = follow(item, crates.flatMap(c => c.files).filter(f => f.root === item.root))
    const seen = new Set<string>()
    const once = (f: Flow) => f.from !== f.to && !seen.has(`${f.from} ${f.to}`) && seen.add(`${f.from} ${f.to}`)
    return { flows: flows.filter(once), exporters }
  }, [item, crates])
  /** With re-exports off, every importer is reached straight from the definition. */
  const flows = useMemo(() => {
    if (reexports) return traced
    const ends = new Set(traced.map(f => f.to).filter(to => to !== home))
    return [...ends].map(to => ({ from: home, to, via: false }))
  }, [traced, reexports, home])

  useEffect(() => {
    node.current?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
    line.current?.scrollIntoView({ block: 'start' })
  }, [picked])

  const shows = (i: Item) => (internal || i.public) && !hidden.has(i.kind)
  /** Listed items in each module and the modules under it. */
  const sizes = useMemo(() => {
    const out = new Map<string, number>()
    const walk = (m: Mod): number => {
      const n = m.items.filter(shows).length + m.children.reduce((sum, c) => sum + walk(c), 0)
      out.set(m.id, n)
      return n
    }
    roots.forEach(walk)
    return out
  }, [roots, internal, hidden])

  if (rows.length === 0) {
    return <p className="body muted">The loaded reports define no items outside test code.</p>
  }

  const at = new Map(rows.map(r => [r.mod.id, r]))
  const bus = Math.max(...rows.map(r => r.x + r.width)) + MARGIN
  const sources = [...new Set(flows.map(f => f.from))]
  /** One path per source module: out to its lane, along it, and back into each importer with a chevron. */
  const wires = sources.flatMap((id, lane) => {
    const from = at.get(id)
    const ends = flows.filter(f => f.from === id).map(f => at.get(f.to)).filter((r): r is Row => r !== undefined)
    if (!from || !ends.length) return []
    const x = bus + lane * LANE
    const ys = [from.y + OUT, ...ends.map(e => e.y + IN)]
    let d = `M${from.x + from.width} ${from.y + OUT}H${x}M${x} ${Math.min(...ys)}V${Math.max(...ys)}`
    for (const e of ends) {
      const [tip, y] = [e.x + e.width + 4, e.y + IN]
      d += `M${x} ${y}H${tip}M${tip + 6} ${y - 4}L${tip} ${y}L${tip + 6} ${y + 4}`
    }
    return [{ id, d, weak: id !== home }]
  })
  const branches = rows
    .map(r => (r.parent ? `M${r.parent.x + INDENT / 2} ${r.parent.y + HEIGHT}V${r.y + HEIGHT / 2}H${r.x}` : ''))
    .join('')
  const width = bus + Math.max(0, sources.length - 1) * LANE + MARGIN
  const height = 2 * MARGIN + rows.length * PITCH - (PITCH - HEIGHT)
  const reached = new Set(flows.flatMap(f => [f.from, f.to]))
  const state = (id: string) => (!home ? '' : id === home ? 'focus' : reached.has(id) ? 'down' : 'off')

  const importers = new Set(traced.map(f => f.to)).size
  const summary = item
    ? `${item.name} is defined in ${item.module}, imported by ${count(importers, 'module')}`
      + (exporters ? ` and re-exported by ${exporters}.` : '.')
    : spot
    ? `${spot.module} defines ${spot.items.length ? count(spot.items.length, 'item') : 'no items'}.`
    : 'Select an item below to see its module and the modules that import it.'

  const flip = <T,>(set: ReadonlySet<T>, v: T) => {
    const next = new Set(set)
    if (!next.delete(v)) next.add(v)
    return next
  }
  const startOpen = all.length <= MAX_OPEN_ITEMS
  /** Focuses a module picked in the tree, opening the list down to it. */
  const visit = (row: Row) => {
    if (row.mod.id === picked) return setPicked(null)
    const next = new Set(toggled)
    // A module is closed when it is toggled from an open start, or untouched from a closed one.
    for (let r: Row | null = row; r; r = r.parent) {
      if (next.has(r.mod.id) === (!r.parent || startOpen) && !next.delete(r.mod.id)) next.add(r.mod.id)
    }
    setToggled(next)
    setPicked(row.mod.id)
  }
  const indent = (depth: number) => ({ '--depth': depth }) as CSSProperties
  const branch = (mod: Mod, depth: number): ReactNode => {
    const size = sizes.get(mod.id) ?? 0
    if (!size) return null
    const open = toggled.has(mod.id) !== (depth === 0 || startOpen)
    return (
      <li key={mod.id}>
        <button
          ref={mod.id === picked ? line : undefined}
          className="line code"
          style={indent(depth)}
          aria-expanded={open}
          aria-current={mod.id === picked || undefined}
          onClick={() => setToggled(flip(toggled, mod.id))}
        >
          <span className="mark">{open ? '−' : '+'}</span>
          {mod.label}
          <span className="faint">{[mod.note, count(size, 'item')].filter(Boolean).join(' · ')}</span>
        </button>
        {open && (
          <ul>
            {mod.children.map(c => branch(c, depth + 1))}
            {mod.items.filter(shows).map(i => (
              <li key={i.id}>
                <button
                  className="line code"
                  style={indent(depth + 1)}
                  aria-pressed={i.id === picked}
                  onClick={() => setPicked(i.id === picked ? null : i.id)}
                >
                  <Tag>{i.kind}</Tag>
                  {i.name}
                  {!i.public && <span className="faint">internal</span>}
                </button>
              </li>
            ))}
          </ul>
        )}
      </li>
    )
  }
  const listed = all.filter(i => internal || i.public)

  return (
    <div className="halves">
      <section className="half" aria-label="Module tree">
        <header className="half-head">
          <p className="small muted" aria-live="polite">{summary}</p>
          {home && <Button size="sm" variant="ghost" onClick={() => setPicked(null)}>Clear focus</Button>}
          <Switch label="Re-exports" checked={reexports} onChange={setReexports} />
        </header>
        <div className="stage">
          <Canvas
            width={width}
            height={height}
            className="tree"
            label={'Module tree. Bright arrows run from the module that defines the focused item to the modules that '
              + 'import it; dimmer arrows run from the modules that re-export it.'}
            start={1}
            onClear={() => setPicked(null)}
          >
            <path d={branches} className="fl-g-line" />
            {/* the definition's own arrows go last, so they draw over the weaker */}
            {[...wires].reverse().map(w => <path key={w.id} d={w.d} className={w.weak ? 'fl-g-edge' : 'fl-g-subject'} />)}
            {rows.map(row => {
              const { mod, x, y, width } = row
              return (
                <g
                  key={mod.id}
                  ref={mod.id === home ? node : undefined}
                  className={`node ${state(mod.id)}`}
                  transform={`translate(${x} ${y})`}
                  role="button"
                  tabIndex={0}
                  aria-pressed={mod.id === picked}
                  onClick={e => {
                    e.stopPropagation()
                    visit(row)
                  }}
                  onKeyDown={e => {
                    if (e.key !== 'Enter' && e.key !== ' ') return
                    e.preventDefault()
                    visit(row)
                  }}
                >
                  <rect width={width} height={HEIGHT} />
                  <text className="fl-g-value" x={width / 2} y={HEIGHT / 2}>
                    {mod.label}
                    {mod.note && <tspan> {mod.note}</tspan>}
                  </text>
                </g>
              )
            })}
          </Canvas>
        </div>
      </section>

      <section className="half" aria-label="Items">
        <header className="half-head">
          <div className="kinds" role="group" aria-label="Kinds shown">
            {KINDS.map(k => [k, listed.filter(i => i.kind === k).length] as const).filter(([, n]) => n > 0).map(([k, n]) => (
              <label key={k} className="code">
                <input type="checkbox" checked={!hidden.has(k)} onChange={() => setHidden(flip(hidden, k))} />
                {k}
                <span>{n}</span>
              </label>
            ))}
          </div>
          <Switch label="Internal items" checked={internal} onChange={setInternal} />
        </header>
        <div className="half-body">
          {roots.some(r => sizes.get(r.id)) ? (
            <ul className="outline">{roots.map(r => branch(r, 0))}</ul>
          ) : (
            <p className="small muted">No item matches. Turn a kind back on, or show internal items.</p>
          )}
        </div>
      </section>
    </div>
  )
}
