import ELK, { type ElkEdgeSection, type ElkNode, type ElkPoint } from 'elkjs/lib/elk.bundled.js'
import { useEffect, useMemo, useState } from 'react'
import { Canvas } from './canvas'
import { Button } from './flint'
import type { Crate } from './report'

const elk = new ELK()

export const DIRECTIONS = [
  { id: 'RIGHT', label: 'Left to right' },
  { id: 'DOWN', label: 'Top down' },
] as const
const ROUTINGS = [
  { id: 'ORTHOGONAL', label: 'Orthogonal' },
  { id: 'POLYLINE', label: 'Polyline' },
  { id: 'SPLINES', label: 'Splines' },
] as const

/** Node height, and the mono advance and side padding its width is estimated from. */
const HEIGHT = 32
const ADVANCE = 7.8
const PAD = 12
/** Length and half-width of an edge's chevron. */
const CHEVRON = [6, 4] as const
/** Edge strokes, weakest to strongest. */
const TIERS = ['fl-g-grid', 'fl-g-edge', 'fl-g-subject']

type Node = { id: string; name: string; version: string; label: string; depth: number; out: Set<string> }
type Box = { id: string; x: number; y: number; width: number }
type Layout = { width: number; height: number; boxes: Box[]; edges: { from: string; to: string; d: string }[] }

const key = (p: { name: string; version: string }) => `${p.name}@${p.version}`

/** Every crate's packages merged by name and version. Depth is the distance from the nearest loaded crate. */
const merge = (crates: Crate[]): Map<string, Node> => {
  const nodes = new Map<string, Node>()
  const names = new Map<string, Set<string>>()
  for (const p of crates.flatMap(c => c.packages)) {
    const id = key(p)
    const node = nodes.get(id) ?? { id, name: p.name, version: p.version, label: p.name, depth: Infinity, out: new Set() }
    p.dependencies.forEach(d => node.out.add(key(d)))
    nodes.set(id, node)
    names.set(p.name, (names.get(p.name) ?? new Set()).add(p.version))
  }
  // Only a name present in several versions needs its version to be told apart.
  for (const node of nodes.values()) if ((names.get(node.name)?.size ?? 0) > 1) node.label = `${node.name} ${node.version}`
  let layer = new Set(crates.map(key))
  for (let depth = 0; layer.size; depth++) {
    const reached = [...layer].map(id => nodes.get(id)).filter((n): n is Node => n !== undefined && n.depth > depth)
    reached.forEach(n => (n.depth = depth))
    layer = new Set(reached.flatMap(n => [...n.out]))
  }
  return nodes
}

const at = (p: ElkPoint) => `${Math.round(p.x)} ${Math.round(p.y)}`

/** One routed edge as path data, ending in an open chevron. Spline bend points are cubic control points. */
const trace = (s: ElkEdgeSection, curved: boolean): string => {
  const points = [...(s.bendPoints ?? []), s.endPoint]
  let d = `M${at(s.startPoint)}`
  let i = 0
  if (curved) for (; i + 2 < points.length; i += 3) d += `C${at(points[i])} ${at(points[i + 1])} ${at(points[i + 2])}`
  for (; i < points.length; i++) d += `L${at(points[i])}`
  const tip = s.endPoint
  const from = points.at(-2) ?? s.startPoint
  const length = Math.hypot(tip.x - from.x, tip.y - from.y)
  if (!length) return d
  const [back, side] = CHEVRON
  const [ux, uy] = [(tip.x - from.x) / length, (tip.y - from.y) / length]
  const arm = (sign: number) => at({ x: tip.x - back * ux + sign * side * uy, y: tip.y - back * uy - sign * side * ux })
  return `${d}M${arm(1)}L${at(tip)}L${arm(-1)}`
}

const read = (out: ElkNode, curved: boolean): Layout => ({
  width: Math.ceil(out.width ?? 0),
  height: Math.ceil(out.height ?? 0),
  boxes: (out.children ?? []).map(c => ({ id: c.id, x: Math.round(c.x ?? 0), y: Math.round(c.y ?? 0), width: c.width ?? 0 })),
  edges: (out.edges ?? []).flatMap(e =>
    (e.sections ?? []).map(s => ({ from: e.sources[0] ?? '', to: e.targets[0] ?? '', d: trace(s, curved) })),
  ),
})

export function Choice<T extends string>({ legend, options, value, onChange }: {
  legend: string
  options: readonly { id: T; label: string }[]
  value: T
  onChange: (id: T) => void
}) {
  return (
    <fieldset className="field">
      <legend className="label">{legend}</legend>
      <div className="seg">
        {options.map(o => (
          <label key={o.id}>
            <input type="radio" name={legend} checked={o.id === value} onChange={() => onChange(o.id)} />
            {o.label}
          </label>
        ))}
      </div>
    </fieldset>
  )
}

export function Dependencies({ crates }: { crates: Crate[] }) {
  const [picked, setPicked] = useState<string | null>(null)
  /** The deepest layer shown; null shows them all. */
  const [limit, setLimit] = useState<number | null>(null)
  const [direction, setDirection] = useState<(typeof DIRECTIONS)[number]['id']>('RIGHT')
  const [routing, setRouting] = useState<(typeof ROUTINGS)[number]['id']>('ORTHOGONAL')
  const [layout, setLayout] = useState<Layout | 'failed' | null>(null)

  const nodes = useMemo(() => merge(crates), [crates])
  const max = useMemo(() => Math.max(1, ...[...nodes.values()].map(n => n.depth).filter(Number.isFinite)), [nodes])
  const depth = Math.min(limit ?? max, max)
  const shown = useMemo(() => new Map([...nodes].filter(([, n]) => n.depth <= depth)), [nodes, depth])

  useEffect(() => {
    let stale = false
    const graph = {
      id: 'root',
      layoutOptions: {
        'elk.algorithm': 'layered',
        'elk.direction': direction,
        'elk.edgeRouting': routing,
        'elk.padding': '[top=24,left=24,bottom=24,right=24]',
        'elk.spacing.nodeNode': '16',
        'elk.layered.spacing.nodeNodeBetweenLayers': '48',
      },
      children: [...shown.values()].map(n => ({
        id: n.id,
        width: 8 * Math.ceil((n.label.length * ADVANCE + 2 * PAD) / 8),
        height: HEIGHT,
      })),
      edges: [...shown.values()].flatMap(n =>
        [...n.out].filter(to => shown.has(to)).map(to => ({ id: `${n.id} ${to}`, sources: [n.id], targets: [to] })),
      ),
    }
    elk.layout(graph).then(
      out => {
        if (!stale) setLayout(read(out, routing === 'SPLINES'))
      },
      () => {
        if (!stale) setLayout('failed')
      },
    )
    return () => {
      stale = true
    }
  }, [shown, direction, routing])

  const drawn = layout !== 'failed' && layout?.width && layout.height ? layout : null
  const focus = picked !== null ? shown.get(picked) : undefined
  /** Everything the focused package reaches through the edges on screen. */
  const downstream = useMemo(() => {
    const seen = new Set<string>()
    const todo = focus ? [focus.id] : []
    for (let id = todo.pop(); id !== undefined; id = todo.pop()) {
      for (const to of shown.get(id)?.out ?? []) {
        if (!shown.has(to) || seen.has(to)) continue
        seen.add(to)
        todo.push(to)
      }
    }
    return seen
  }, [focus, shown])

  if (nodes.size === 0) {
    return (
      <p className="body muted">
        The loaded reports hold no resolved dependencies. Regenerate them where cargo can resolve the project; it may
        need network access.
      </p>
    )
  }

  /** An edge's strength: 2 on the focused package, 1 between its dependencies, 0 elsewhere. All 1 with no focus. */
  const tier = (e: { from: string; to: string }) =>
    !focus ? 1 : e.from === focus.id || e.to === focus.id ? 2 : downstream.has(e.from) ? 1 : 0
  const toggle = (id: string) => setPicked(id === focus?.id ? null : id)

  return (
    <>
      <div className="stage">
        {layout === 'failed' ? (
          <p className="errors small" role="alert">
            The graph could not be laid out. Change the depth or the edge routing to try again.
          </p>
        ) : drawn && (
          <Canvas
            width={drawn.width}
            height={drawn.height}
            className="dag"
            label="Dependency graph. Edges point from a package to the packages it depends on."
            onClear={() => setPicked(null)}
          >
            {/* stronger edges go last, so they draw over the weaker */}
            {[...drawn.edges].sort((a, b) => tier(a) - tier(b)).map(e => (
              <path key={`${e.from} ${e.to}`} d={e.d} className={TIERS[tier(e)]} />
            ))}
            {drawn.boxes.map(box => {
              const node = shown.get(box.id)
              if (!node) return null
              const state = node === focus ? 'focus' : downstream.has(node.id) ? 'down' : focus ? 'off' : ''
              return (
                <g
                  key={node.id}
                  className={`node ${state}`}
                  transform={`translate(${box.x} ${box.y})`}
                  role="button"
                  tabIndex={0}
                  aria-pressed={node === focus}
                  aria-label={`${node.name} ${node.version}`}
                  onClick={e => {
                    e.stopPropagation()
                    toggle(node.id)
                  }}
                  onKeyDown={e => {
                    if (e.key !== 'Enter' && e.key !== ' ') return
                    e.preventDefault()
                    toggle(node.id)
                  }}
                >
                  <rect width={box.width} height={HEIGHT} />
                  <text className="fl-g-value" x={box.width / 2} y={HEIGHT / 2}>
                    {node.name}
                    {node.label !== node.name && <tspan> {node.version}</tspan>}
                  </text>
                </g>
              )
            })}
          </Canvas>
        )}
      </div>

      <aside className="panel" aria-label="Graph options">
        <section aria-live="polite">
          <h2 className="label">Focused package</h2>
          {focus ? (
            <>
              <div>
                <div className="heading name">{focus.name}</div>
                <div className="code muted">{focus.version}</div>
              </div>
              <Button size="sm" onClick={() => setPicked(null)}>Clear focus</Button>
            </>
          ) : (
            <p className="small muted">Select a package in the graph to focus it and what it depends on.</p>
          )}
        </section>

        <section>
          <h2 className="heading">Filter</h2>
          <label className="field">
            <span className="between">
              <span className="label">Max depth</span>
              <span className="code">{depth} of {max}</span>
            </span>
            <input
              className="range"
              type="range"
              min={1}
              max={max}
              value={depth}
              disabled={max === 1}
              onChange={e => setLimit(Number(e.target.value) >= max ? null : Number(e.target.value))}
            />
          </label>
          <p className="small muted">{shown.size} of {nodes.size} packages shown.</p>
        </section>

        <section>
          <h2 className="heading">Format</h2>
          <Choice legend="Direction" options={DIRECTIONS} value={direction} onChange={setDirection} />
          <Choice legend="Edge routing" options={ROUTINGS} value={routing} onChange={setRouting} />
        </section>
      </aside>
    </>
  )
}
