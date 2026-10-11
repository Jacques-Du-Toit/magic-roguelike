// The act's map: rows of nodes from the first row at the bottom to the
// boss at the top, with the paths between them.

import type { MapNode } from '../engine/types/MapNode.ts'
import type { RunView } from '../engine/types/RunView.ts'
import { NODE_LABEL } from '../run/describe.ts'

interface RunMapProps {
  view: RunView
  /** Travel to a node the engine offers. */
  onTravel: (node: number) => void
}

export function RunMap({ view, onTravel }: RunMapProps) {
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
        const label = NODE_LABEL[node.kind]
        const canGo = reachable.has(node.id)
        const state = node.id === here ? 'here' : visited.has(node.id) ? 'visited' : canGo ? 'open' : 'far'
        const said =
          state === 'here'
            ? ', you are here'
            : state === 'visited'
              ? ', visited'
              : state === 'open'
                ? ', can go here'
                : ''
        return (
          <button
            key={node.id}
            type="button"
            className={`map-node map-node-${node.kind} map-node-${state}`}
            style={{
              left: `${(x(node) / map.columns) * 100}%`,
              top: `${(y(node) / map.rows) * 100}%`,
            }}
            disabled={!canGo}
            onClick={() => onTravel(node.id)}
            title={label.about}
            aria-label={`${label.name}${said}`}
            data-node={node.id}
            data-kind={node.kind}
            data-state={state}
          >
            <span className="map-node-symbol" aria-hidden="true">
              {label.symbol}
            </span>
            <span className="map-node-name">{label.name}</span>
            {state === 'here' && <span className="map-node-mark">You</span>}
            {state === 'visited' && <span className="map-node-mark">Done</span>}
          </button>
        )
      })}
    </div>
  )
}
