import { Fragment, useMemo, useState } from 'react'
import { BarChart, Card } from './flint'
import { type Crate, type Totals, sum } from './report'

type Metrics = Omit<Totals, 'files'>

/** Line kinds in chart-colour order: the n-th kind is always chart-n. */
const KINDS = [
  { key: 'code', label: 'Code' },
  { key: 'test', label: 'Test' },
  { key: 'doc', label: 'Doc' },
  { key: 'comment', label: 'Comment' },
  { key: 'blank', label: 'Blank' },
] as const

/** Crates drawn in the lines chart before the smallest are folded into one bar. */
const MAX_BARS = 12
/** Crates start expanded in the table up to this many files in total. */
const MAX_OPEN_FILES = 50

const total = (counts: Record<string, number>) => Object.values(counts).reduce((a, b) => a + b, 0)

const COLUMNS = [
  { id: 'lines', label: 'Lines', of: (m: Metrics) => total(m.lines) },
  { id: 'code', label: 'Code', of: (m: Metrics) => m.lines.code },
  { id: 'test', label: 'Test', of: (m: Metrics) => m.lines.test },
  { id: 'doc', label: 'Doc', of: (m: Metrics) => m.lines.doc },
  { id: 'tests', label: 'Tests', of: (m: Metrics) => m.tests.count },
  { id: 'items', label: 'Items', of: (m: Metrics) => total(m.items) },
]

const number = new Intl.NumberFormat()
const fmt = (v: number) => number.format(v)
const share = (part: number, whole: number) => {
  if (!whole || !part) return undefined
  const pct = (100 * part) / whole
  return pct > 0 && pct < 1 ? '<1%' : `${Math.round(pct)}%`
}

function Tile({ label, value, note, slot }: { label: string; value: number | string; note?: string; slot?: number }) {
  return (
    <div className="tile">
      <dt className="label">
        {slot && <span className={`fl-chart-sw fl-chart-c${slot}`} />}
        {label}
      </dt>
      <dd className={value === 0 ? 'title zero' : 'title'}>
        {typeof value === 'number' ? fmt(value) : value}
        {note && <span className="code">{note}</span>}
      </dd>
    </div>
  )
}

/** One bar split by line kind; the neighbouring cells carry the numbers. */
function Mix({ lines }: { lines: Totals['lines'] }) {
  const whole = total(lines)
  const text = KINDS.filter(k => lines[k.key] > 0).map(k => `${k.label} ${share(lines[k.key], whole)}`).join(', ')
  return (
    <div className="fl-chart-stack ratio" role="img" aria-label={text || 'No lines'} title={text}>
      {KINDS.map((k, i) =>
        lines[k.key] > 0 && <div key={k.key} className={`fl-chart-seg fl-chart-c${i + 1}`} style={{ flexGrow: lines[k.key] }} />,
      )}
    </div>
  )
}

function Modules({ crates, all }: { crates: Crate[]; all: Totals }) {
  const [sort, setSort] = useState({ by: 'lines', desc: true })
  const [toggled, setToggled] = useState<ReadonlySet<string>>(new Set())
  const startOpen = all.files <= MAX_OPEN_FILES

  const groups = useMemo(() => {
    const column = COLUMNS.find(c => c.id === sort.by)
    const dir = sort.desc ? -1 : 1
    const rank = (a: Metrics, aName: string, b: Metrics, bName: string) =>
      dir * (column ? column.of(a) - column.of(b) : aName.localeCompare(bName))
    return [...crates]
      .sort((a, b) => rank(a.totals, a.name, b.totals, b.name))
      .map(crate => ({
        crate,
        files: [...crate.files].sort((a, b) => rank(a, a.module + a.path, b, b.module + b.path)),
      }))
  }, [crates, sort])

  const head = (id: string, label: string) => {
    const on = sort.by === id
    return (
      <th key={id} scope="col" aria-sort={on ? (sort.desc ? 'descending' : 'ascending') : undefined}>
        <button className="sort label" onClick={() => setSort({ by: id, desc: on ? !sort.desc : id !== 'name' })}>
          {label}
          {on && (sort.desc ? ' ↓' : ' ↑')}
        </button>
      </th>
    )
  }
  const cells = (m: Metrics) => (
    <>
      <td><Mix lines={m.lines} /></td>
      {COLUMNS.map(c => {
        const v = c.of(m)
        return <td key={c.id} className={v ? undefined : 'zero'}>{fmt(v)}</td>
      })}
    </>
  )
  const toggle = (name: string) =>
    setToggled(prev => {
      const next = new Set(prev)
      if (!next.delete(name)) next.add(name)
      return next
    })

  return (
    <Card eyebrow="Crates and files · lines and counts" title="Modules">
      <div className="mods">
        <table>
          <thead>
            <tr>
              {head('name', 'Module')}
              <th scope="col"><span className="label">Line mix</span></th>
              {COLUMNS.map(c => head(c.id, c.label))}
            </tr>
          </thead>
          <tbody>
            {groups.map(({ crate, files }) => {
              const open = toggled.has(crate.name) !== startOpen
              const root = `${crate.path}/`
              return (
                <Fragment key={crate.name}>
                  <tr className="crate">
                    <th scope="row">
                      <button className="fold" aria-expanded={open} onClick={() => toggle(crate.name)}>
                        <span className="mark">{open ? '−' : '+'}</span>
                        {crate.name}
                      </button>
                      <span className="path">
                        {crate.path} · {files.length === 1 ? '1 file' : `${fmt(files.length)} files`}
                      </span>
                    </th>
                    {cells(crate.totals)}
                  </tr>
                  {open && files.map(f => (
                    <tr key={f.path}>
                      <th scope="row">
                        {f.module}
                        <span className="path">{f.path.startsWith(root) ? f.path.slice(root.length) : f.path}</span>
                      </th>
                      {cells(f)}
                    </tr>
                  ))}
                </Fragment>
              )
            })}
          </tbody>
        </table>
      </div>
    </Card>
  )
}

export function Overview({ crates }: { crates: Crate[] }) {
  const all = useMemo(() => sum(crates.map(c => c.totals)), [crates])
  const lines = total(all.lines)
  const errors = total(all.error_handling)

  // Dependencies on other loaded crates are workspace-internal.
  const external = useMemo(() => {
    const local = new Set(crates.map(c => c.name))
    const deps = crates.flatMap(c => c.dependencies).filter(d => !(d.source === 'path' && local.has(d.name)))
    return new Set(deps.map(d => d.name)).size
  }, [crates])

  // Largest crates first; past MAX_BARS the smallest share one bar.
  const bars = useMemo(() => {
    const ranked = [...crates].sort((a, b) => total(b.totals.lines) - total(a.totals.lines))
    if (ranked.length <= MAX_BARS) return ranked.map(c => ({ name: c.name, lines: c.totals.lines }))
    const rest = ranked.slice(MAX_BARS - 1)
    return [
      ...ranked.slice(0, MAX_BARS - 1).map(c => ({ name: c.name, lines: c.totals.lines })),
      { name: `${rest.length} other crates`, lines: sum(rest.map(c => c.totals)).lines },
    ]
  }, [crates])

  const items = Object.entries(all.items).sort((a, b) => b[1] - a[1])
  const tested = crates.flatMap(c => c.files).filter(f => f.tests.count > 0).length

  return (
    <>
      <dl className="tiles">
        <Tile label="Crates" value={crates.length} />
        <Tile label="Files" value={all.files} />
        <Tile label="Lines" value={lines} />
        <Tile label="Items" value={total(all.items)} />
        <Tile label="Tests" value={all.tests.count} />
        <Tile label="Dependencies" value={external} note="external" />
      </dl>

      <Card eyebrow="Size · lines" title="Lines by kind">
        <dl className="tiles tiles-5">
          {KINDS.map((k, i) => (
            <Tile key={k.key} slot={i + 1} label={k.label} value={all.lines[k.key]} note={share(all.lines[k.key], lines)} />
          ))}
        </dl>
        <BarChart
          className="share"
          orientation="horizontal"
          stacked
          legend={false}
          label="Lines by kind for each crate"
          categoryLabel="Crate"
          categories={bars.map(b => b.name)}
          series={KINDS.map(k => ({ name: k.label, values: bars.map(b => b.lines[k.key]) }))}
        />
      </Card>

      <div className="split">
        <Card eyebrow="Definitions · count" title="Items by kind">
          <BarChart
            orientation="horizontal"
            label="Item definitions by kind"
            categoryLabel="Kind"
            categories={items.map(([kind]) => kind)}
            series={[{ name: 'Items', values: items.map(([, count]) => count) }]}
          />
        </Card>

        <div className="quad">
          <Card eyebrow="Test fns and files · count" title="Tests">
            <dl className="tiles tiles-2">
              <Tile label="Ignored" value={all.tests.ignored} note={`of ${fmt(all.tests.count)}`} />
              <Tile label="Tested files" value={tested} note={`of ${fmt(all.files)}`} />
              <Tile
                label="Test : code"
                value={all.lines.code ? (all.lines.test / all.lines.code).toFixed(2) : '–'}
                note="lines"
              />
            </dl>
          </Card>
          <Card eyebrow="Call sites · count" title="Error handling">
            <dl className="tiles tiles-2">
              <Tile label="Propagate ?" value={all.error_handling.propagate} note={share(all.error_handling.propagate, errors)} />
              <Tile label="Unwrap" value={all.error_handling.unwrap} note={share(all.error_handling.unwrap, errors)} />
              <Tile label="Expect" value={all.error_handling.expect} note={share(all.error_handling.expect, errors)} />
              <Tile label="Panic" value={all.error_handling.panic} note={share(all.error_handling.panic, errors)} />
            </dl>
          </Card>
          <Card eyebrow="Markers · count" title="Suppressions">
            <dl className="tiles tiles-2">
              <Tile label="Todo" value={all.suppressions.todo} />
              <Tile label="Unimplemented" value={all.suppressions.unimplemented} />
              <Tile label="Unreachable" value={all.suppressions.unreachable} />
              <Tile label="Allow" value={all.suppressions.allow} />
            </dl>
          </Card>
          <Card eyebrow="Uses · count" title="Functional style">
            <dl className="tiles tiles-2">
              <Tile label="Closures" value={all.functional.closures} />
              <Tile label="Combinators" value={all.functional.combinators} />
              <Tile label="Higher-order" value={all.functional.higher_order_fns} note="fns" />
            </dl>
          </Card>
        </div>
      </div>

      <Modules crates={crates} all={all} />
    </>
  )
}
