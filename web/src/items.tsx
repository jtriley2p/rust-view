import { type KeyboardEvent, type MouseEvent, useMemo, useState } from 'react'
import { Canvas } from './canvas'
import { Button } from './flint'
import { type Crate, type Definition, type File, KINDS, type Kind } from './report'

/** A module's header height and least width, an item line's height, and the space under a module's last item. */
const HEIGHT = 40
const WIDTH = 128
const LINE = 28
const FOOT = 8
/** Space between modules, and indent per tree level. */
const GAP = 8
const INDENT = 32
/** Clear space inside the drawing's edges, and between the widest module and the first arrow lane. */
const MARGIN = 24
/** Distance between the lanes arrows run down, one per place they leave. */
const LANE = 16
/** Where arrows enter and leave a module, below its top edge. */
const IN = 12
const OUT = 28
/** The mono advance and side padding widths are estimated from, and the characters an item's kind column takes. */
const ADVANCE = 7.8
const PAD = 16
const KIND = 8

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
/** A module as drawn. `items` are those on show: none while it is closed; `count` is how many it would show open. */
type Row = { mod: Mod; x: number; y: number; width: number; height: number; parent: Row | null; items: Item[]; count: number }
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

/** What makes an SVG group a button: a press, or Enter or Space, runs `act`. */
const press = (act: () => void) => ({
  role: 'button',
  tabIndex: 0,
  onClick: (e: MouseEvent) => {
    e.stopPropagation()
    act()
  },
  onKeyDown: (e: KeyboardEvent) => {
    if (e.key !== 'Enter' && e.key !== ' ') return
    e.preventDefault()
    act()
  },
})

export function Items({ crates }: { crates: Crate[] }) {
  /** The focused item's id. */
  const [picked, setPicked] = useState<string | null>(null)
  /** The modules opened to show their items. */
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set())
  const [reexports, setReexports] = useState(true)
  const [internal, setInternal] = useState(false)
  const [hidden, setHidden] = useState<ReadonlySet<Kind>>(new Set())

  const roots = useMemo(() => build(crates), [crates])
  const rows = useMemo(() => {
    const out: Row[] = []
    let y = MARGIN
    const walk = (mod: Mod, depth: number, parent: Row | null) => {
      const listed = mod.items.filter(i => (internal || i.public) && !hidden.has(i.kind))
      const items = open.has(mod.id) ? listed : []
      // The header holds a mark, the label, and the count; a line holds the kind, the name, and ` internal`.
      const chars = Math.max(
        mod.label.length + (mod.note ? mod.note.length + 1 : 0) + 4 + String(listed.length).length,
        ...items.map(i => KIND + i.name.length + (i.public ? 0 : 9)),
      )
      const row = {
        mod,
        parent,
        items,
        count: listed.length,
        x: MARGIN + depth * INDENT,
        y,
        width: Math.max(WIDTH, 8 * Math.ceil((chars * ADVANCE + 2 * PAD) / 8)),
        height: HEIGHT + (items.length ? items.length * LINE + FOOT : 0),
      }
      y += row.height + GAP
      out.push(row)
      mod.children.forEach(c => walk(c, depth + 1, row))
    }
    roots.forEach(r => walk(r, 0, null))
    return out
  }, [roots, open, internal, hidden])
  /** Only an item on show holds the focus. */
  const item = useMemo(() => rows.flatMap(r => r.items).find(i => i.id === picked), [rows, picked])
  const home = item ? key(item.root, item.module) : ''

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

  if (rows.length === 0) {
    return <p className="body muted">The loaded reports define no items outside test code.</p>
  }

  const at = new Map(rows.map(r => [r.mod.id, r]))
  const bus = Math.max(...rows.map(r => r.x + r.width)) + MARGIN
  const sources = [...new Set(flows.map(f => f.from))]
  /** One path per source: out to its lane, along it, and back into each importer with a chevron. */
  const wires = sources.flatMap((id, lane) => {
    const from = at.get(id)
    const ends = flows.filter(f => f.from === id).map(f => at.get(f.to)).filter((r): r is Row => r !== undefined)
    if (!from || !ends.length) return []
    // The definition's arrows leave from the item's own line; a re-export's leave from its module's header.
    const line = id === home && item ? from.items.indexOf(item) : -1
    const out = from.y + (line < 0 ? OUT : HEIGHT + line * LINE + LINE / 2)
    const x = bus + lane * LANE
    const ys = [out, ...ends.map(e => e.y + IN)]
    let d = `M${from.x + from.width} ${out}H${x}M${x} ${Math.min(...ys)}V${Math.max(...ys)}`
    for (const e of ends) {
      const [tip, y] = [e.x + e.width + 4, e.y + IN]
      d += `M${x} ${y}H${tip}M${tip + 6} ${y - 4}L${tip} ${y}L${tip + 6} ${y + 4}`
    }
    return [{ id, d, weak: id !== home }]
  })
  const branches = rows
    .map(r => (r.parent ? `M${r.parent.x + INDENT / 2} ${r.parent.y + r.parent.height}V${r.y + HEIGHT / 2}H${r.x}` : ''))
    .join('')
  const width = bus + Math.max(0, sources.length - 1) * LANE + MARGIN
  const height = Math.max(...rows.map(r => r.y + r.height)) + MARGIN
  const reached = new Set(flows.flatMap(f => [f.from, f.to]))
  const state = (id: string) => (!item ? '' : id === home ? 'focus' : reached.has(id) ? 'down' : 'off')

  const importers = new Set(traced.map(f => f.to)).size
  const summary = item
    ? `${item.name} is defined in ${item.module}, imported by ${count(importers, 'module')}`
      + (exporters ? ` and re-exported by ${exporters}.` : '.')
    : 'Open a module to see its items, then select one to see the modules that import it.'

  const flip = <T,>(set: ReadonlySet<T>, v: T) => {
    const next = new Set(set)
    if (!next.delete(v)) next.add(v)
    return next
  }
  const listed = rows.flatMap(r => r.mod.items).filter(i => internal || i.public)

  return (
    <div className="frame">
      <header className="bar">
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
        <Switch label="Re-exports" checked={reexports} onChange={setReexports} />
      </header>
      <div className="bar">
        <p className="small muted" aria-live="polite">{summary}</p>
        {item && <Button size="sm" variant="ghost" onClick={() => setPicked(null)}>Clear focus</Button>}
      </div>
      <div className="stage">
        <Canvas
          width={width}
          height={height}
          className="tree"
          label={'Module tree. A module opens to list its items. Bright arrows run from the focused item to the '
            + 'modules that import it; dimmer arrows run from the modules that re-export it.'}
          start={1}
          onClear={() => setPicked(null)}
        >
          <path d={branches} className="fl-g-line" />
          {/* the definition's own arrows go last, so they draw over the weaker */}
          {[...wires].reverse().map(w => <path key={w.id} d={w.d} className={w.weak ? 'fl-g-edge' : 'fl-g-subject'} />)}
          {rows.map(({ mod, x, y, width, height, items, count }) => (
            <g key={mod.id} className={`node ${state(mod.id)}`} transform={`translate(${x} ${y})`}>
              <rect width={width} height={height} />
              <g
                className="head"
                {...(count > 0 && { ...press(() => setOpen(flip(open, mod.id))), 'aria-expanded': open.has(mod.id) })}
              >
                <rect className="hit" width={width} height={HEIGHT} />
                {count > 0 && <text className="fl-g-value faint" x={PAD} y={HEIGHT / 2}>{open.has(mod.id) ? '−' : '+'}</text>}
                <text className="fl-g-value" x={PAD + 2 * ADVANCE} y={HEIGHT / 2}>
                  {mod.label}
                  {mod.note && <tspan> {mod.note}</tspan>}
                </text>
                {count > 0 && <text className="fl-g-value faint end" x={width - PAD} y={HEIGHT / 2}>{count}</text>}
              </g>
              {items.map((i, n) => (
                <g
                  key={i.id}
                  className="item"
                  transform={`translate(0 ${HEIGHT + n * LINE})`}
                  aria-pressed={i === item}
                  {...press(() => setPicked(i === item ? null : i.id))}
                >
                  <rect className="hit" x={2} width={width - 4} height={LINE} />
                  <text className="fl-g-value faint" x={PAD} y={LINE / 2}>{i.kind}</text>
                  <text className="fl-g-value" x={PAD + KIND * ADVANCE} y={LINE / 2}>
                    {i.name}
                    {!i.public && <tspan> internal</tspan>}
                  </text>
                </g>
              ))}
            </g>
          ))}
        </Canvas>
      </div>
    </div>
  )
}
