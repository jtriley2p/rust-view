// Types and loading for the CLI's report (crates/core/spec/report.schema.json).

export const VERSION = 2

/** Every count at zero: the shape reports are checked against and the seed for sums. */
export const ZERO = {
  files: 0,
  lines: { code: 0, test: 0, doc: 0, comment: 0, blank: 0 },
  tests: { count: 0, ignored: 0 },
  items: {
    struct: 0, enum: 0, union: 0, trait: 0, impl: 0, fn: 0, method: 0,
    const: 0, static: 0, type: 0, mod: 0, macro: 0,
  },
  functional: { closures: 0, combinators: 0, higher_order_fns: 0 },
  suppressions: { todo: 0, unimplemented: 0, unreachable: 0, allow: 0 },
  error_handling: { unwrap: 0, expect: 0, panic: 0, propagate: 0 },
}

export type Totals = typeof ZERO
export type File = Omit<Totals, 'files'> & { path: string; module: string }
export type Dependency = {
  name: string
  req: string | null
  kind: 'normal' | 'dev' | 'build'
  source: 'registry' | 'path' | 'git'
}
export type Crate = { name: string; path: string; totals: Totals; dependencies: Dependency[]; files: File[] }

export type LoadError =
  | { kind: 'read'; file: string }
  | { kind: 'parse'; file: string }
  | { kind: 'version'; file: string; found: unknown }
  | { kind: 'shape'; file: string; at: string }

export const describe = (e: LoadError): string => {
  switch (e.kind) {
    case 'read': return `${e.file} could not be read. Check the file and open it again.`
    case 'parse': return `${e.file} is not JSON. Open a report written by the rust-view CLI.`
    case 'version': return `${e.file} is schema version ${String(e.found)}. Regenerate it with a CLI that writes version ${VERSION}.`
    case 'shape': return `${e.file} does not match the report schema at ${e.at}. Regenerate it with the current CLI.`
  }
}

type Json = Record<string, unknown>
const isObject = (v: unknown): v is Json => typeof v === 'object' && v !== null && !Array.isArray(v)

/** The first path where `v` lacks a count that `shape` has, if any. */
const mismatch = (v: unknown, shape: object, at: string): string | null => {
  if (!isObject(v)) return at
  for (const [key, want] of Object.entries(shape)) {
    const here = `${at}.${key}`
    const bad = typeof want === 'number'
      ? (Number.isFinite(v[key]) ? null : here)
      : mismatch(v[key], want, here)
    if (bad) return bad
  }
  return null
}

const { files: _, ...FILE_ZERO } = ZERO

/** The crates of one parsed report, or the first place it departs from the schema. */
const crates = (file: string, report: unknown): Crate[] | LoadError => {
  if (!isObject(report)) return { kind: 'shape', file, at: 'the top level' }
  if (report.version !== VERSION) return { kind: 'version', file, found: report.version }
  if (!Array.isArray(report.crates)) return { kind: 'shape', file, at: 'crates' }
  for (const [i, c] of report.crates.entries()) {
    const at = `crates[${i}]`
    if (!isObject(c) || typeof c.name !== 'string' || typeof c.path !== 'string') return { kind: 'shape', file, at }
    if (!Array.isArray(c.files) || !Array.isArray(c.dependencies)) return { kind: 'shape', file, at }
    const bad = mismatch(c.totals, ZERO, `${at}.totals`) ?? c.files
      .map((f, j) => {
        const here = `${at}.files[${j}]`
        return isObject(f) && typeof f.path === 'string' && typeof f.module === 'string' ? mismatch(f, FILE_ZERO, here) : here
      })
      .find(Boolean)
    if (bad) return { kind: 'shape', file, at: bad }
  }
  return report.crates as Crate[]
}

/** Reports in `text`: one JSON document, or one per line as the CLI prints to stdout. */
const documents = (text: string): unknown[] | null => {
  try {
    return [JSON.parse(text)]
  } catch {
    try {
      return text.split('\n').filter(line => line.trim()).map(line => JSON.parse(line))
    } catch {
      return null
    }
  }
}

export type Loaded = { crates: Crate[]; errors: LoadError[] }

/** Merges every report in the given files. A crate listed twice keeps its last report. */
export const load = (inputs: { name: string; text: string | null }[]): Loaded => {
  const byName = new Map<string, Crate>()
  const errors: LoadError[] = []
  for (const { name, text } of inputs) {
    const docs = text === null ? null : documents(text)
    if (text === null) errors.push({ kind: 'read', file: name })
    else if (!docs || docs.length === 0) errors.push({ kind: 'parse', file: name })
    for (const doc of docs ?? []) {
      const found = crates(name, doc)
      if (Array.isArray(found)) found.forEach(c => byName.set(c.name, c))
      else errors.push(found)
    }
  }
  return { crates: [...byName.values()], errors }
}

const add = <T>(a: T, b: T): T =>
  typeof a === 'number'
    ? ((a + (b as number)) as T)
    : (Object.fromEntries(Object.entries(a as object).map(([k, v]) => [k, add(v, (b as Json)[k])])) as T)

export const sum = (totals: Totals[]): Totals => totals.reduce(add, ZERO)
