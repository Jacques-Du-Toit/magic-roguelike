// The column beside the battlefield: the stack and the game log.

import { useEffect, useRef } from 'react'
import { HUMAN } from '../engine/index.ts'
import type { CardDef } from '../engine/types/CardDef.ts'
import type { PlayerView } from '../engine/types/PlayerView.ts'
import type { StackItemView } from '../engine/types/StackItemView.ts'
import type { Target } from '../engine/types/Target.ts'
import { stackItemDef, stackItemName } from '../duel/names.ts'
import type { LogEntry } from '../duel/present.ts'
import { ManaCost } from './Card.tsx'
import { usePreview } from './usePreview.ts'

interface StackProps {
  view: PlayerView
  cards: ReadonlyMap<string, CardDef>
  targetName: (target: Target) => string
  isTarget: (object: number) => boolean
  onItem: (item: StackItemView) => void
}

function StackItem({
  item,
  top,
  cards,
  targetName,
  isTarget,
  onItem,
}: Omit<StackProps, 'view'> & { item: StackItemView; top: boolean }) {
  const def = stackItemDef(item, cards)
  const preview = usePreview(def)
  return (
    <li>
      <button
        type="button"
        className={`stack-item${isTarget(item.id) ? ' lit-target' : ''}`}
        data-object={item.id}
        onClick={() => onItem(item)}
        {...preview}
      >
        <span className="stack-item-title">
          <span>{stackItemName(item, cards)}</span>
          {item.kind.type === 'spell' && def && <ManaCost cost={def.mana_cost} />}
        </span>
        <span className="stack-item-detail">
          {item.controller === HUMAN ? 'Yours' : "Opponent's"}
          {top ? ' · resolves next' : ''}
        </span>
        {item.targets.length > 0 && (
          <span className="stack-item-detail">Targets {item.targets.map(targetName).join(', ')}</span>
        )}
      </button>
    </li>
  )
}

export function StackPanel({ view, ...rest }: StackProps) {
  // The top of the stack is the end of the list; show it first.
  const items = view.stack.toReversed()
  return (
    <section className="panel stack-panel" aria-label="Stack">
      <h2>Stack</h2>
      {items.length === 0 ? (
        <p className="muted">Empty</p>
      ) : (
        <ol>
          {items.map((item, index) => (
            <StackItem key={item.id} item={item} top={index === 0} {...rest} />
          ))}
        </ol>
      )}
    </section>
  )
}

export function LogPanel({ log }: { log: LogEntry[] }) {
  const end = useRef<HTMLDivElement>(null)
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'nearest' })
  }, [log.length])
  return (
    <section className="panel log-panel" aria-label="Game log">
      <h2>Log</h2>
      <div className="log-lines" data-testid="log">
        {log.map((entry, index) => (
          <p key={index} className={`log-${entry.kind}`}>
            {entry.text}
          </p>
        ))}
        <div ref={end} />
      </div>
    </section>
  )
}
