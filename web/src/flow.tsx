import ELK, { type ElkNode, type ElkPoint } from 'elkjs/lib/elk.bundled.js'
import { type ReactNode, useEffect, useMemo, useState } from 'react'
import { Canvas } from './canvas'
import { Choice, DIRECTIONS } from './dependencies'
import { Button, Input } from './flint'
import { flip, press } from './items'
import { type Crate, MODES, type Mode, type Place } from './report'

const elk = new ELK()

const WALKS = [
  { id: 'both', label: 'Both' },
  { id: 'forward', label: 'Forward' },
  { id: 'backward', label: 'Backward' },
] as const
type Walk = (typeof WALKS)[number]['id']

/** The furthest a slice reaches, and where it starts: a type's functions and what they connect it to. */
const HOPS = { max: 6, start: 2 }
/** The most types and functions the picker lists. */
const PICKS = 8
/** The height of a type and of a function, and the mono advance and side padding a width is estimated from. */
const HEIGHT = { type: 32, fn: 24 }
const ADVANCE = 7.8
const PAD = 12
/** A module label's height, and the advance of its smaller, spaced capitals. */
const LABEL = { height: 16, advance: 7.2 }
/** An arrowhead's length and half-width; a read's square terminal is as long on each side. */
const HEAD = [6, 4] as const

/** How an arrow's type is taken or given; a conversion joins two types with no function between. */
const KINDS = [...MODES, 'convert'] as const
type Kind = Mode | 'convert'
/** Each kind's name, what it says of the type, and its stroke. A conversion keeps the plain stroke. */
const KIND: Record<Kind, { label: string; note: string; stroke: string }> = {
  move: { label: 'Move', note: 'by value', stroke: 'fl-g-s1' },
  read: { label: 'Read', note: 'by &', stroke: 'fl-g-s2' },
  mut: { label: 'Mutate', note: 'by &mut, changed in place', stroke: 'fl-g-s3' },
  convert: { label: 'Convert', note: 'From, TryFrom, or Into', stroke: 'fl-g-edge' },
}

/** A type or a function. */
type Node = {
  id: string
  fn: boolean
  /** The name; an outside type's follows its path within its crate, a function's ends in `()`. */
  label: string
  /** The module as shown: the crate's name for `crate`, the path as written for an outside type. */
  module: string
  /** The box the node is drawn in: its module, or its crate if outside the loaded ones. */
  cluster: string
  group: string
  external: boolean
  /** How many arrows touch it, errors aside, and how many reach it as an error. */
  degree: number
  errors: number
}
/** An arrow from a type into a function, from a function to a type it gives, or between two types. */
type Edge = { id: string; from: string; to: string; kind: Kind; error: boolean; channel: boolean }
/** What an arrow is hidden by: its kind, or a mark any kind can carry. */
type Trait = Kind | 'error' | 'channel'
const hides = (hidden: ReadonlySet<Trait>, e: Edge) =>
  hidden.has(e.kind) || (e.error && hidden.has('error')) || (e.channel && hidden.has('channel'))
/** `out` and `into` are the arrows a walk leaves a node by, forward and backward. */
type Graph = { nodes: Map<string, Node>; out: Map<string, Edge[]>; into: Map<string, Edge[]> }

/** Every crate's flows merged. A node of the loaded crates is keyed by its crate, target, module, and name. */
const build = (crates: Crate[]): Graph => {
  const graph: Graph = { nodes: new Map(), out: new Map(), into: new Map() }
  const edges = new Set<string>()
  const node = (id: string, rest: Omit<Node, 'id' | 'degree' | 'errors'>): Node => {
    const made = graph.nodes.get(id) ?? { id, ...rest, degree: 0, errors: 0 }
    graph.nodes.set(id, made)
    return made
  }
  const link = (map: Map<string, Edge[]>, id: string, e: Edge) => map.set(id, [...(map.get(id) ?? []), e])
  const add = (from: Node, to: Node, kind: Kind, error: boolean, channel: boolean) => {
    const id = `${from.id} > ${to.id} ${kind} ${error} ${channel}`
    if (from === to || edges.has(id)) return
    edges.add(id)
    const e = { id, from: from.id, to: to.id, kind, error, channel }
    link(graph.out, e.from, e)
    link(graph.into, e.to, e)
    // What is changed in place also takes what the function is given.
    if (kind === 'mut') {
      link(graph.out, e.to, e)
      link(graph.into, e.from, e)
    }
    if (error) to.errors++
    else for (const end of [from, to]) end.degree++
  }
  for (const crate of crates) {
    for (const file of crate.files) {
      const local = (module: string) => ({
        module: module.replace(/^crate/, crate.name),
        cluster: `${crate.name} ${file.root} ${module}`,
        group: module.replace(/^crate/, crate.name),
        external: false,
      })
      const type = (p: Place): Node => {
        if (p.file !== null) return node(`${local(p.module).cluster} ${p.name}`, { fn: false, label: p.name, ...local(p.module) })
        const cut = p.module.indexOf('::')
        const home = cut < 0 ? p.module : p.module.slice(0, cut)
        const label = cut < 0 ? p.name : `${p.module.slice(cut + 2)}::${p.name}`
        return node(`outside ${home} ${label}`, {
          fn: false, label, module: p.module, cluster: `outside ${home}`, group: `${home} (external)`, external: true,
        })
      }
      for (const f of file.flows) {
        if (f.kind === 'fn') {
          const fn = node(`${local(f.module).cluster} ${f.name}()`, { fn: true, label: `${f.name}()`, ...local(f.module) })
          for (const p of f.inputs) add(type(p.type), fn, p.by, false, p.channel)
          for (const p of f.outputs) add(fn, type(p.type), p.by, p.error, p.channel)
          continue
        }
        for (const i of f.inputs) {
          for (const o of f.outputs) add(type(i.type), type(o.type), f.kind === 'convert' ? 'convert' : i.by, o.error, i.channel || o.channel)
        }
      }
    }
  }
  return graph
}

/**
 * The arrows within `hops` of `focus`, forward and backward walked apart so a hub does not join the two.
 * A walk stops at an outside type, whose own flows are unknown. An error is a branch off the flow: none is
 * walked across, and only those of the focus and of the functions that take it are kept. Nor is a hidden arrow.
 */
const slice = (graph: Graph, focus: string, hops: number, walk: Walk, hidden: ReadonlySet<Trait>): Edge[] => {
  const shown = new Set<Edge>()
  const go = (forward: boolean) => {
    const seen = new Set([focus])
    let layer = [focus]
    for (let hop = 0; hop < hops && layer.length; hop++) {
      const next: string[] = []
      for (const id of layer) {
        for (const e of (forward ? graph.out : graph.into).get(id) ?? []) {
          if (hides(hidden, e) || (e.error && hop > Number(forward))) continue
          shown.add(e)
          const end = e.from === id ? e.to : e.from
          if (seen.has(end) || graph.nodes.get(end)?.external || e.error) continue
          seen.add(end)
          next.push(end)
        }
      }
      layer = next
    }
  }
  if (walk !== 'backward') go(true)
  if (walk !== 'forward') go(false)
  return [...shown]
}

type Wire = { edge: Edge; line: string; head: string }
type Box = { id: string; x: number; y: number; width: number; height: number }
type Frame = Box & { label: string; at: { x: number; y: number } }
type Layout = { width: number; height: number; frames: Frame[]; boxes: Box[]; wires: Wire[] }

const at = (p: ElkPoint) => `${Math.round(p.x)} ${Math.round(p.y)}`

/**
 * An edge's line and arrowheads as path data. A move ends in a chevron, a conversion in a chevron behind a bar,
 * and a read in a square terminal; a mutation has a chevron at each end. A channel is two bars across the middle
 * of the line.
 */
const draw = (kind: Kind, channel: boolean, points: ElkPoint[]): { line: string; head: string } => {
  const [start, tip] = [points[0], points.at(-1)]
  const before = points.findLast(p => p.x !== tip?.x || p.y !== tip?.y)
  const after = points.find(p => p.x !== start?.x || p.y !== start?.y)
  if (!start || !tip || !before || !after) return { line: '', head: '' }
  const [back, side] = HEAD
  const along = (a: ElkPoint, b: ElkPoint) => {
    const length = Math.hypot(b.x - a.x, b.y - a.y) || 1
    return { x: (b.x - a.x) / length, y: (b.y - a.y) / length, length }
  }
  /** The point `behind` a point along a direction and `aside` of it. */
  const off = (p: ElkPoint, u: ElkPoint, behind: number, aside: number) =>
    at({ x: p.x - behind * u.x - aside * u.y, y: p.y - behind * u.y + aside * u.x })
  const chevron = (p: ElkPoint, u: ElkPoint) => `M${off(p, u, back, side)}L${at(p)}L${off(p, u, back, -side)}`
  const bar = (p: ElkPoint, v: ElkPoint, behind: number) => `M${off(p, v, behind, side)}L${off(p, v, behind, -side)}`
  const u = along(before, tip)
  let head = chevron(tip, u)
  if (kind === 'mut') head += chevron(start, along(after, start))
  if (kind === 'convert') head += bar(tip, u, back + 3)
  if (kind === 'read') head = `M${off(tip, u, back, side - 1)}L${off(tip, u, 0, side - 1)}L${off(tip, u, 0, 1 - side)}L${off(tip, u, back, 1 - side)}Z`
  // The line stops at a terminal's near side.
  const end = kind === 'read' ? off(tip, u, back, 0) : at(tip)
  const line = points.slice(0, -1).map((p, i) => `${i ? 'L' : 'M'}${at(p)}`).join('') + `L${end}`
  if (!channel) return { line, head }
  const total = points.slice(1).reduce((sum, p, i) => sum + along(points[i]!, p).length, 0)
  let left = total / 2
  for (const [i, p] of points.slice(1).entries()) {
    const v = along(points[i]!, p)
    if (left > v.length) {
      left -= v.length
      continue
    }
    const mid = { x: points[i]!.x + left * v.x, y: points[i]!.y + left * v.y }
    head += bar(mid, v, -2) + bar(mid, v, 2)
    break
  }
  return { line, head }
}

const read = (out: ElkNode, edges: Map<string, Edge>): Layout => {
  const box = (n: ElkNode) => ({ id: n.id, x: Math.round(n.x ?? 0), y: Math.round(n.y ?? 0), width: n.width ?? 0, height: n.height ?? 0 })
  return {
    width: Math.ceil(out.width ?? 0),
    height: Math.ceil(out.height ?? 0),
    frames: (out.children ?? []).map(c => ({
      ...box(c),
      label: c.labels?.[0]?.text ?? '',
      at: { x: Math.round(c.labels?.[0]?.x ?? 0), y: Math.round(c.labels?.[0]?.y ?? 0) },
    })),
    boxes: (out.children ?? []).flatMap(c => (c.children ?? []).map(box)),
    wires: (out.edges ?? []).flatMap(e => {
      const [edge, s] = [edges.get(e.id), e.sections?.[0]]
      return edge && s ? [{ edge, ...draw(edge.kind, edge.channel, [s.startPoint, ...(s.bendPoints ?? []), s.endPoint]) }] : []
    }),
  }
}

/** The legend's sample of an arrow. With no kind, it shows a mark any kind can carry, in the plain stroke. */
function Key({ kind, error = false, channel = false }: { kind?: Kind; error?: boolean; channel?: boolean }) {
  const { line, head } = draw(kind ?? 'move', channel, [{ x: 2, y: 8 }, { x: 46, y: 8 }])
  const stroke = kind ? KIND[kind].stroke : 'fl-g-edge'
  return (
    <svg className="fl-gfx key" viewBox="0 0 48 16" aria-hidden="true">
      <path d={line} className={`${stroke}${error ? ' fl-g-dashed' : ''}`} />
      <path d={head} className={stroke} />
    </svg>
  )
}

export function Flow({ crates }: { crates: Crate[] }) {
  const [picked, setPicked] = useState<string | null>(null)
  /** The node kept in the drawing, and where it is on screen, while the focus moves. */
  const [pinned, setPinned] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [hops, setHops] = useState(HOPS.start)
  const [walk, setWalk] = useState<Walk>('both')
  const [hidden, setHidden] = useState<ReadonlySet<Trait>>(new Set())
  const [direction, setDirection] = useState<(typeof DIRECTIONS)[number]['id']>('RIGHT')
  const [layout, setLayout] = useState<Layout | 'failed' | null>(null)

  const graph = useMemo(() => build(crates), [crates])
  const ranked = useMemo(() => [...graph.nodes.values()].sort((a, b) => b.degree - a.degree), [graph])
  /** The picked node, or to begin with the busiest type of the loaded crates that is no error. */
  const focus = (picked !== null ? graph.nodes.get(picked) : undefined)
    ?? ranked.find(n => !n.fn && !n.external && n.errors === 0)
    ?? ranked[0]
  const pin = pinned !== null ? graph.nodes.get(pinned) : undefined
  /** The focus's slice and, with it, the pinned node's: where the two meet, the drawing joins them. */
  const edges = useMemo(() => {
    const starts = [...new Set([focus, pin])].filter((n): n is Node => n !== undefined)
    return [...new Set(starts.flatMap(n => slice(graph, n.id, hops, walk, hidden)))]
  }, [graph, focus, pin, hops, walk, hidden])
  const shown = useMemo(() => {
    const ids = new Set(edges.flatMap(e => [e.from, e.to]))
    for (const n of [focus, pin]) if (n) ids.add(n.id)
    return [...ids].map(id => graph.nodes.get(id)).filter((n): n is Node => n !== undefined)
  }, [graph, edges, focus, pin])

  useEffect(() => {
    let stale = false
    const groups = new Map<string, Node[]>()
    for (const n of shown) groups.set(n.cluster, [...(groups.get(n.cluster) ?? []), n])
    const inset = `[top=${PAD},left=${PAD},bottom=${PAD},right=${PAD}]`
    const width = (chars: number, advance: number) => 8 * Math.ceil((chars * advance + 2 * PAD) / 8)
    const input = {
      id: 'root',
      layoutOptions: {
        'elk.algorithm': 'layered',
        'elk.direction': direction,
        'elk.edgeRouting': 'ORTHOGONAL',
        // Modules are laid out with their nodes, and every position is given from the drawing's corner.
        'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
        'elk.json.shapeCoords': 'ROOT',
        'elk.json.edgeCoords': 'ROOT',
        'elk.padding': '[top=24,left=24,bottom=24,right=24]',
        'elk.spacing.nodeNode': '16',
        'elk.spacing.edgeNode': '16',
        'elk.layered.spacing.nodeNodeBetweenLayers': '56',
        'elk.layered.spacing.edgeNodeBetweenLayers': '16',
      },
      children: [...groups].map(([id, nodes]) => ({
        id,
        layoutOptions: { 'elk.padding': inset, 'elk.nodeLabels.placement': 'INSIDE V_TOP H_LEFT', 'elk.nodeLabels.padding': inset },
        labels: [{ text: nodes[0]?.group ?? '', width: width(nodes[0]?.group.length ?? 0, LABEL.advance) - 2 * PAD, height: LABEL.height }],
        children: nodes.map(n => ({ id: n.id, width: width(n.label.length, ADVANCE), height: n.fn ? HEIGHT.fn : HEIGHT.type })),
      })),
      edges: edges.map(e => ({ id: e.id, sources: [e.from], targets: [e.to] })),
    }
    const byId = new Map(edges.map(e => [e.id, e]))
    elk.layout(input).then(
      out => {
        if (!stale) setLayout(read(out, byId))
      },
      () => {
        if (!stale) setLayout('failed')
      },
    )
    return () => {
      stale = true
    }
  }, [shown, edges, direction])

  if (!focus) {
    return (
      <p className="body muted">
        The loaded reports hold no flows. A flow needs a fn that takes or gives one of the crate's types.
      </p>
    )
  }

  const drawn = layout !== 'failed' && layout?.width && layout.height ? layout : null
  const held = drawn?.boxes.find(b => b.id === pin?.id)
  const count = (nodes: Iterable<Node>, fn: boolean) => [...nodes].filter(n => n.fn === fn).length
  const focusOn = (id: string) => {
    setPicked(id)
    setQuery('')
  }
  /** A legend row, which is also the switch that shows or hides its arrows. */
  const row = (id: Trait, label: string, note: string, key: ReactNode) => (
    <li key={id}>
      <label className="keyed small switch">
        {key}
        <span>{label} <span className="muted">{note}</span></span>
        <input type="checkbox" role="switch" checked={!hidden.has(id)} onChange={() => setHidden(flip(hidden, id))} />
      </label>
    </li>
  )
  const wanted = query.trim().toLowerCase()
  const matches = wanted ? ranked.filter(n => `${n.module}::${n.label}`.toLowerCase().includes(wanted)) : []

  return (
    <>
      <div className="stage">
        {layout === 'failed' ? (
          <p className="errors small" role="alert">
            The graph could not be laid out. Lower the hops or pick another type to try again.
          </p>
        ) : drawn && (
          <Canvas
            width={drawn.width}
            height={drawn.height}
            className="dag flow"
            label={`Data flow around ${focus.label}. Arrows point from the types a function takes into it, and from it `
              + 'to the types it gives, boxed by module. Select a type or a function to centre the graph on it.'}
            hold={held && { x: held.x + held.width / 2, y: held.y + held.height / 2 }}
            onClear={() => {}}
          >
            {drawn.frames.map(f => (
              <g key={f.id}>
                <rect className="fl-g-line" x={f.x} y={f.y} width={f.width} height={f.height} />
                <text className="fl-g-label" x={f.at.x} y={f.at.y + LABEL.height / 2}>{f.label}</text>
              </g>
            ))}
            {drawn.wires.map(({ edge, line, head }) => (
              <g key={edge.id}>
                <path className={`${KIND[edge.kind].stroke}${edge.error ? ' fl-g-dashed' : ''}`} d={line} />
                <path className={KIND[edge.kind].stroke} d={head} />
              </g>
            ))}
            {drawn.boxes.map(box => {
              const node = graph.nodes.get(box.id)
              if (!node) return null
              return (
                <g
                  key={box.id}
                  className={`node${node.fn ? ' fn' : ''}${node === focus ? ' focus' : ''}${node === pin ? ' pinned' : ''}`}
                  transform={`translate(${box.x} ${box.y})`}
                  aria-pressed={node === focus}
                  aria-label={`${node.fn ? 'Function' : 'Type'} ${node.label} in ${node.module}${node === pin ? ', pinned' : ''}`}
                  {...press(() => focusOn(node.id))}
                >
                  <rect width={box.width} height={box.height} />
                  <text className="fl-g-value" x={box.width / 2} y={box.height / 2}>{node.label}</text>
                  {node === pin && <path className="mark" d={`M${box.width - 8} 3h5v5h-5z`} />}
                </g>
              )
            })}
          </Canvas>
        )}
      </div>

      <aside className="panel" aria-label="Flow options">
        <section>
          <h2 className="label">Focused {focus.fn ? 'function' : 'type'}</h2>
          <div aria-live="polite">
            <div className="heading name">{focus.label}</div>
            <div className="code muted name">{focus.module}</div>
          </div>
          <Button size="sm" onClick={() => setPinned(focus === pin ? null : focus.id)}>{focus === pin ? 'Unpin' : 'Pin'}</Button>
          {pin && pin !== focus && <p className="small muted">{pin.label} is pinned, with its own slice. Pinning this moves the pin here.</p>}
          <Input label="Find type or function" type="search" placeholder="Name or module" value={query} onChange={e => setQuery(e.target.value)} />
          {wanted && (matches.length === 0 ? (
            <p className="small muted">Nothing matches. Only types and functions with a flow are listed.</p>
          ) : (
            <>
              <ul className="picks">
                {matches.slice(0, PICKS).map(n => (
                  <li key={n.id}>
                    <button type="button" className="pick" onClick={() => focusOn(n.id)}>
                      <span className="code">{n.label}</span>
                      <span className="small muted">{n.module}</span>
                    </button>
                  </li>
                ))}
              </ul>
              {matches.length > PICKS && <p className="small muted">{matches.length - PICKS} more. Type more to narrow them.</p>}
            </>
          ))}
        </section>

        <section>
          <h2 className="heading">Slice</h2>
          <label className="field">
            <span className="between">
              <span className="label">Hops</span>
              <span className="code">{hops}</span>
            </span>
            <input className="range" type="range" min={1} max={HOPS.max} value={hops} onChange={e => setHops(Number(e.target.value))} />
          </label>
          <Choice legend="Walk" options={WALKS} value={walk} onChange={setWalk} />
          <p className="small muted">
            {count(shown, false)} of {count(graph.nodes.values(), false)} types and {count(shown, true)} of{' '}
            {count(graph.nodes.values(), true)} functions shown. A hop is one arrow. A walk stops at an external type,
            and shows errors next to its start only.
          </p>
        </section>

        <section>
          <h2 className="heading">Arrows</h2>
          <p className="small muted">
            A function, named with (), sits between the types it takes and the types it gives. Switch a kind off to
            hide its arrows; a walk does not cross a hidden arrow.
          </p>
          <ul className="plain">
            {KINDS.map(k => row(k, KIND[k].label, KIND[k].note, <Key kind={k} />))}
            {row('error', 'Error', 'the E of a Result', <Key error />)}
            {row('channel', 'Channel', 'sent, then received', <Key channel />)}
          </ul>
        </section>

        <section>
          <h2 className="heading">Format</h2>
          <Choice legend="Direction" options={DIRECTIONS} value={direction} onChange={setDirection} />
        </section>
      </aside>
    </>
  )
}
