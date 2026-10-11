// Hover-to-enlarge: one large copy of the card under the pointer, placed
// beside it.

import { type ReactNode, useMemo, useState } from 'react'
import { CardFrame } from './Card.tsx'
import { type PreviewApi, PreviewContext, type Shown } from './usePreview.ts'

// Matches `.card-large` in index.css.
const PREVIEW_WIDTH = 260
const PREVIEW_HEIGHT = 364
const GAP = 12

export function PreviewProvider({ children }: { children: ReactNode }) {
  const [shown, setShown] = useState<Shown | null>(null)
  const api = useMemo<PreviewApi>(
    () => ({
      show: setShown,
      hide: (owner) => setShown((current) => (current?.owner === owner ? null : current)),
    }),
    [],
  )

  let style: { left: number; top: number } | undefined
  if (shown) {
    const { anchor } = shown
    const clamp = (value: number, size: number, room: number) =>
      Math.min(Math.max(GAP, value), Math.max(GAP, room - size - GAP))
    if (anchor.top - GAP - PREVIEW_HEIGHT >= 0 && anchor.bottom > window.innerHeight * 0.75) {
      // A card along the bottom edge, e.g. in hand: show it above, where it
      // doesn't cover the cards beside it.
      style = {
        left: clamp(anchor.left + anchor.width / 2 - PREVIEW_WIDTH / 2, PREVIEW_WIDTH, window.innerWidth),
        top: anchor.top - GAP - PREVIEW_HEIGHT,
      }
    } else {
      const fitsRight = anchor.right + GAP + PREVIEW_WIDTH <= window.innerWidth
      style = {
        left: fitsRight ? anchor.right + GAP : Math.max(GAP, anchor.left - GAP - PREVIEW_WIDTH),
        top: clamp(anchor.top + anchor.height / 2 - PREVIEW_HEIGHT / 2, PREVIEW_HEIGHT, window.innerHeight),
      }
    }
  }

  return (
    <PreviewContext.Provider value={api}>
      {children}
      {shown && (
        <div className="preview" style={style} data-testid="card-preview">
          <CardFrame def={shown.def} size="large" />
        </div>
      )}
    </PreviewContext.Provider>
  )
}
