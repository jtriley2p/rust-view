// The report store the CLI keeps beside the built page: `index.js` lists the projects and `reports/<id>.js` holds
// one's reports. They are scripts that hand their JSON to `rustView`, since a file:// page can run a script beside
// it but cannot fetch or list files.

/** One project in the store. */
export type Entry = { id: string; name: string; path: string; time: number; version: number; crates: string[] }

/** Whether the page is the one the CLI installed, and so has a store beside it. */
export const STORED = location.protocol === 'file:'

let index: unknown = null
const reports = new Map<string, unknown>()
;(window as unknown as { rustView: object }).rustView = {
  index: (entries: unknown) => { index = entries },
  report: (id: string, docs: unknown) => { reports.set(id, docs) },
}

/** Runs a store script. A missing or broken one hands nothing over. */
const run = (src: string) =>
  new Promise<void>(done => {
    const script = document.createElement('script')
    // The time keeps a rewritten script from being served from memory.
    script.src = `${src}?${Date.now()}`
    script.onload = script.onerror = () => {
      script.remove()
      done()
    }
    document.head.append(script)
  })

const isEntry = (v: unknown): v is Entry => {
  const e = v as Entry | null
  return typeof e === 'object' && e !== null
    && typeof e.id === 'string' && typeof e.name === 'string' && typeof e.path === 'string'
    && typeof e.time === 'number' && typeof e.version === 'number' && Array.isArray(e.crates)
}

/** The store's projects, newest first. */
export const list = async (): Promise<Entry[]> => {
  if (!STORED) return []
  index = null
  await run('index.js')
  return Array.isArray(index) ? index.filter(isEntry).reverse() : []
}

/** One project's reports, or `null` if its script is gone or is not one. */
export const read = async (id: string): Promise<unknown[] | null> => {
  await run(`reports/${id}.js`)
  const docs = reports.get(id)
  reports.delete(id)
  return Array.isArray(docs) ? docs : null
}
