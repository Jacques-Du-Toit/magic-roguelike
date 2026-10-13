// The act's map: rows of nodes from the first row at the bottom to the
// boss at the top, with the paths between them. An elite's node names its
// commander and the boss's node names the boss: the engine says who waits
// at each (`MapNode.enemy`), and nobody waits at any other node.

import type { CardDef } from '../engine/types/CardDef.ts'
import type { EnemyDef } from '../engine/types/EnemyDef.ts'
import type { MapNode } from '../engine/types/MapNode.ts'
import type { RunView } from '../engine/types/RunView.ts'
import { NODE_LABEL } from '../run/describe.ts'
import { usePreview } from './usePreview.ts'

interface RunMapProps {
  view: RunView
  enemies: readonly EnemyDef[]
  cards: ReadonlyMap<string, CardDef>
  /** Travel to a node the engine offers. */
  onTravel: (node: number) => void
}

type NodeState = 'here' | 'visited' | 'open' | 'far'

const SAID: Record<NodeState, string> = {
  here: ', you are here',
  visited: ', visited',
  open: ', can go here',
  far: '',
}

interface MapSpotProps {
  node: MapNode
  state: NodeState
  /** Who waits here, for an elite and the boss. */
  enemy: EnemyDef | undefined
  commander: CardDef | undefined
  left: string
  top: string
  onTravel: (node: number) => void
}

function MapSpot({ node, state, enemy, commander, left, top, onTravel }: MapSpotProps) {
  const label = NODE_LABEL[node.kind]
  // Hovering shows the commander's card. The handlers are on the spot, not
  // the button: a node that can't be gone to yet is a disabled button,
  // which hears no mouse.
  const preview = usePreview(commander)
  // The boss is known by its own name, an elite by its commander's.
  const shown = node.kind === 'boss' ? enemy?.name : commander?.name
  const who = enemy
    ? [
        `${enemy.name}.`,
        commander ? `Commander: ${commander.name}.` : '',
        ...enemy.modifiers.map((modifier) => `${modifier.name}: ${modifier.text}`),
      ]
        .filter((part) => part !== '')
        .join(' ')
    : ''
  return (
    <div
      className="map-spot"
      style={{ left, top }}
      title={who ? `${who} ${label.about}` : label.about}
      {...preview}
    >
      <button
        type="button"
        className={`map-node map-node-${node.kind} map-node-${state}`}
        disabled={state !== 'open'}
        onClick={() => onTravel(node.id)}
        aria-label={`${label.name}${who ? `: ${who}` : ''}${SAID[state]}`}
        data-node={node.id}
        data-kind={node.kind}
        data-state={state}
        data-enemy={enemy?.id}
        data-commander={commander?.id}
      >
        {shown ? (
          <>
            <span className="map-node-name">
              <span aria-hidden="true">{label.symbol}</span> {label.name}
            </span>
            <span className="map-node-enemy" data-testid="map-enemy">
              {shown}
            </span>
          </>
        ) : (
          <>
            <span className="map-node-symbol" aria-hidden="true">
              {label.symbol}
            </span>
            <span className="map-node-name">{label.name}</span>
          </>
        )}
        {state === 'here' && <span className="map-node-mark">You</span>}
        {state === 'visited' && <span className="map-node-mark">Done</span>}
      </button>
    </div>
  )
}

export function RunMap({ view, enemies, cards, onTravel }: RunMapProps) {
  const { map, path } = view
  const reachable = new Set(
    view.actions.flatMap((action) => (action.type === 'travel' ? [action.node] : [])),
  )
  const visited = new Set(path)
  const here = path.at(-1)
  const boss = map.rows - 1

  // Positions in grid units: x across the columns, y from the top. The boss
  // sits in the middle of its row.
  const x = (node: MapNode) => (node.row === boss ? map.columns / 2 : node.column + 0.5)
  const y = (node: MapNode) => map.rows - 1 - node.row + 0.5
  const byId = new Map(map.nodes.map((node) => [node.id, node]))

  return (
    <div
      className="run-map"
      style={{ aspectRatio: `${map.columns * 1.9} / ${map.rows}` }}
      data-testid="run-map"
    >
      <svg
        className="run-map-paths"
        viewBox={`0 0 ${map.columns} ${map.rows}`}
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        {map.nodes.flatMap((node) =>
          node.next.map((next) => {
            const target = byId.get(next)
            if (!target) return null
            const walked = visited.has(node.id) && visited.has(next)
            const open = node.id === here && reachable.has(next)
            return (
              <line
                key={`${node.id}-${next}`}
                x1={x(node)}
                y1={y(node)}
                x2={x(target)}
                y2={y(target)}
                className={walked ? 'path-walked' : open ? 'path-open' : 'path'}
              />
            )
          }),
        )}
      </svg>
      {map.nodes.map((node) => {
        const state: NodeState =
          node.id === here
            ? 'here'
            : visited.has(node.id)
              ? 'visited'
              : reachable.has(node.id)
                ? 'open'
                : 'far'
        const enemy = enemies.find((def) => def.id === node.enemy)
        return (
          <MapSpot
            key={node.id}
            node={node}
            state={state}
            enemy={enemy}
            commander={enemy?.commander ? cards.get(enemy.commander) : undefined}
            left={`${(x(node) / map.columns) * 100}%`}
            top={`${(y(node) / map.rows) * 100}%`}
            onTravel={onTravel}
          />
        )
      })}
    </div>
  )
}
