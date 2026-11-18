import { type PointerEvent, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Button } from './flint'

/** Scale bounds, and the factor one press of a zoom button steps by. */
const ZOOM = { min: 0.1, max: 2, step: 1.25 }
/** Pointer travel in px before a press pans the canvas instead of clicking. */
const SLOP = 4

type Point = { x: number; y: number }

/**
 * An SVG drawing that zooms and pans, with its zoom buttons; it fills a `.stage`.
 * `onClear` is a press on the background, or Escape.
 */
export function Canvas({ width, height, className, label, start = 'fit', hold, onClear, children }: {
  width: number
  height: number
  className: string
  label: string
  /** The zoom to open at. */
  start?: number | 'fit'
  /** A drawing point kept where it is on screen while the drawing around it changes. */
  hold?: Point | undefined
  onClear: () => void
  children: ReactNode
}) {
  /** A fixed scale, or `fit`: the whole drawing in view, never enlarged. */
  const [zoom, setZoom] = useState(start)
  const [canvas, setCanvas] = useState<HTMLDivElement | null>(null)
  const [view, setView] = useState({ width: 0, height: 0 })
  const svg = useRef<SVGSVGElement>(null)
  /** A drawing point to put back under a canvas point once the next scale has rendered. */
  const anchor = useRef<{ x: number; y: number; gx: number; gy: number } | null>(null)

  useEffect(() => {
    if (!canvas) return
    const measure = () => setView({ width: canvas.clientWidth, height: canvas.clientHeight })
    const observer = new ResizeObserver(measure)
    observer.observe(canvas)
    measure()
    return () => observer.disconnect()
  }, [canvas])

  const fit = view.width && view.height ? Math.min(1, view.width / width, view.height / height) : 1
  const scale = zoom === 'fit' ? fit : zoom

  /** Where the drawing sits in the canvas: its scroll position, or its margin when it is the smaller. */
  const offset = () => {
    const [outer, inner] = [canvas?.getBoundingClientRect(), svg.current?.getBoundingClientRect()]
    return outer && inner ? { left: inner.left - outer.left, top: inner.top - outer.top } : null
  }
  /** Zooms to `next`, holding still the drawing point under canvas point (x, y); the centre by default. */
  const zoomTo = (next: number, x = view.width / 2, y = view.height / 2) => {
    const [from, to] = [offset(), Math.min(ZOOM.max, Math.max(Math.min(ZOOM.min, fit), next))]
    // An unchanged zoom draws nothing, and would leave its anchor for a later one.
    if (to === zoom) return
    if (from) anchor.current = { x, y, gx: (x - from.left) / scale, gy: (y - from.top) / scale }
    setZoom(to)
  }
  /** The drawing's place and scale and the held point, as last drawn or scrolled. */
  const seen = useRef<{ left: number; top: number; scale: number; hold: Point | undefined } | null>(null)
  const see = () => {
    const at = offset()
    seen.current = at && { ...at, scale, hold }
  }
  useLayoutEffect(() => {
    const [to, zoomed, was] = [offset(), anchor.current, seen.current]
    anchor.current = null
    if (!canvas || !to) return
    // Newly held or let go, the point is drawn where it was; only the margins around the drawing change.
    const [now, from] = [hold ?? was?.hold, was?.hold ?? hold]
    if (zoomed) {
      canvas.scrollLeft += to.left + zoomed.gx * scale - zoomed.x
      canvas.scrollTop += to.top + zoomed.gy * scale - zoomed.y
    } else if (was && now && from) {
      canvas.scrollLeft += to.left + now.x * scale - (was.left + from.x * was.scale)
      canvas.scrollTop += to.top + now.y * scale - (was.top + from.y * was.scale)
    }
    see()
  })

  // Pinch, or wheel with ctrl or cmd, zooms about the pointer. React's own wheel listener is passive and
  // can't stop the browser zooming the page instead; this one is re-added each render to see the current scale.
  useEffect(() => {
    if (!canvas) return
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return
      e.preventDefault()
      const box = canvas.getBoundingClientRect()
      zoomTo(scale * Math.exp(-e.deltaY / 100), e.clientX - box.left, e.clientY - box.top)
    }
    canvas.addEventListener('wheel', onWheel, { passive: false })
    return () => canvas.removeEventListener('wheel', onWheel)
  })

  /** A press on the canvas: where it and the scroll position began, and whether it has become a pan. */
  const drag = useRef({ x: 0, y: 0, left: 0, top: 0, state: 'idle' as 'idle' | 'down' | 'panning' | 'done' })
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    const el = e.currentTarget
    const box = el.getBoundingClientRect()
    // Touch pans natively, and a press past the client area is on a scrollbar.
    const inside = e.clientX - box.left < el.clientWidth && e.clientY - box.top < el.clientHeight
    if (e.button !== 0 || e.pointerType === 'touch' || !inside) return
    drag.current = { x: e.clientX, y: e.clientY, left: el.scrollLeft, top: el.scrollTop, state: 'down' }
  }
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current
    // The release was missed (it landed outside the window, say): don't pan an unpressed pointer.
    if (e.buttons !== 1 && d.state !== 'done') d.state = 'idle'
    const [dx, dy] = [e.clientX - d.x, e.clientY - d.y]
    if (d.state === 'down' && Math.hypot(dx, dy) >= SLOP) {
      // Capture keeps the pan going when the pointer leaves the canvas.
      e.currentTarget.setPointerCapture(e.pointerId)
      d.state = 'panning'
    }
    if (d.state !== 'panning') return
    e.currentTarget.scrollLeft = d.left - dx
    e.currentTarget.scrollTop = d.top - dy
  }
  /** A finished pan stays `done` until its click has been swallowed, or the next press. */
  const onPointerEnd = () => {
    drag.current.state = drag.current.state === 'panning' ? 'done' : 'idle'
  }

  return (
    <>
      <div
        ref={setCanvas}
        className={zoom === 'fit' && !hold ? 'canvas' : 'canvas pan'}
        onScroll={see}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerEnd}
        onPointerCancel={onPointerEnd}
        onClickCapture={e => {
          if (drag.current.state !== 'done') return
          e.stopPropagation()
          drag.current.state = 'idle'
        }}
        onClick={onClear}
        onKeyDown={e => e.key === 'Escape' && onClear()}
      >
        <svg
          ref={svg}
          className={`fl-gfx graph ${className}`}
          role="group"
          aria-label={label}
          viewBox={`0 0 ${width} ${height}`}
          // A held point can be anywhere in the drawing: a canvas of margin on each side lets it scroll to any place.
          style={{
            width: Math.floor(width * scale),
            height: Math.floor(height * scale),
            ...(hold && { margin: `${view.height}px ${view.width}px` }),
          }}
        >
          {children}
        </svg>
      </div>
      <div className="zoom" role="group" aria-label="Zoom">
        <Button size="sm" variant="ghost" aria-label="Zoom out" disabled={scale <= Math.min(ZOOM.min, fit)} onClick={() => zoomTo(scale / ZOOM.step)}>
          −
        </Button>
        <Button size="sm" variant="ghost" aria-label="Zoom to 100%" onClick={() => zoomTo(1)}>
          {Math.round(scale * 100)}%
        </Button>
        <Button size="sm" variant="ghost" aria-label="Zoom in" disabled={scale >= ZOOM.max} onClick={() => zoomTo(scale * ZOOM.step)}>
          +
        </Button>
        <Button size="sm" variant="ghost" aria-pressed={zoom === 'fit'} onClick={() => setZoom('fit')}>
          Fit
        </Button>
      </div>
    </>
  )
}
