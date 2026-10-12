#!/usr/bin/env node
// Adds a batch of cards from a spec file: each card's facts come from
// Scryfall (`scryfall.mjs add`), and the spec supplies what is written by
// hand: the abilities, and anything else to set on the file (`enchant`,
// the cube's own `rarity`, `altered` with our `oracle_text`...). Tokens
// the cards make are written too.
//
//   node tools/add-cards.mjs spec.json
//
// The spec:
//   {
//     "cards": {
//       "Centaur Healer": { "abilities": [ ... ] },
//       "Armada Wurm": { "rarity": "rare", "abilities": [ ... ] }
//     },
//     "tokens": [
//       { "id": "wurm_token", "name": "Wurm", "colors": ["G"], "subtypes": ["Wurm"],
//         "power": 5, "toughness": 5, "keywords": ["trample"] }
//     ]
//   }
//
// A card whose file exists already is not fetched again: the spec's fields
// are set on the file as it is. Afterwards run `cargo test` (the loader
// validates every file) and `node tools/scryfall.mjs check`.

import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { cardId } from './scryfall.mjs'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const write = (path, value) => writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`)

/** A token's file: a creature with no cost, as `data/tokens` holds them. */
export function tokenFile(token) {
  const { id, name, colors, subtypes, power, toughness, keywords } = token
  if (!id || !name || power === undefined || toughness === undefined) {
    throw new Error(`a token needs id, name, power and toughness: ${JSON.stringify(token)}`)
  }
  return {
    id,
    name,
    mana_cost: '',
    colors: colors ?? [],
    types: ['Creature'],
    subtypes: subtypes ?? [name],
    power,
    toughness,
    rarity: 'common',
    ...(keywords?.length ? { keywords } : {}),
  }
}

/** The card file with the spec's fields set on it, abilities last. */
export function withSpec(file, spec) {
  const { abilities: _old, ...rest } = file
  const { abilities, ...fields } = spec
  return { ...rest, ...fields, abilities: abilities ?? [] }
}

function main() {
  const [specPath] = process.argv.slice(2)
  if (!specPath) {
    console.error('usage: node tools/add-cards.mjs spec.json')
    process.exit(2)
  }
  const spec = JSON.parse(readFileSync(specPath, 'utf8'))
  for (const [name, fields] of Object.entries(spec.cards ?? {})) {
    const id = cardId(name)
    const path = join(root, 'data', 'cards', `${id}.json`)
    if (!existsSync(path)) {
      execFileSync(process.execPath, [join(root, 'tools', 'scryfall.mjs'), 'add', name], {
        stdio: ['ignore', 'ignore', 'inherit'],
      })
    }
    const file = withSpec(JSON.parse(readFileSync(path, 'utf8')), fields)
    write(path, file)
    console.log(`${id}: ${file.mana_cost} ${file.types.join(' ')} (${file.rarity})`)
  }
  for (const token of spec.tokens ?? []) {
    write(join(root, 'data', 'tokens', `${token.id}.json`), tokenFile(token))
    console.log(`${token.id}: token`)
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main()
