// The hook side of hover-to-enlarge; `Preview.tsx` draws the enlarged card.

import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import type { CardDef } from '../engine/types/CardDef.ts'

export interface Shown {
  def: CardDef
  anchor: DOMRect
  /** Identifies who asked, so only they can hide it. */
  owner: object
}

export interface PreviewApi {
  show: (shown: Shown) => void
  hide: (owner: object) => void
}

export const PreviewContext = createContext<PreviewApi | null>(null)

/** Props to spread on an element so hovering it enlarges `def`. */
export function usePreview(def: CardDef | undefined) {
  const api = useContext(PreviewContext)
  const [owner] = useState(() => ({}))
  // A card that is clicked away never gets a mouseleave.
  useEffect(() => () => api?.hide(owner), [api, owner])

  const onMouseEnter = useCallback(
    (event: React.MouseEvent<HTMLElement>) => {
      if (def) api?.show({ def, anchor: event.currentTarget.getBoundingClientRect(), owner })
    },
    [api, def, owner],
  )
  const onMouseLeave = useCallback(() => api?.hide(owner), [api, owner])
  // Clicking a card puts the enlarged copy away, so it can't cover whatever
  // is clicked next (a target, say).
  return { onMouseEnter, onMouseLeave, onMouseDown: onMouseLeave }
}
