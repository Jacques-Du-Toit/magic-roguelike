// Card data from Scryfall. No dependencies; needs Node 20 or later.
//
//   node tools/scryfall.mjs add "Card Name" ["Another Card" ...]
//       Fetches each card and writes data/cards/<id>.json with everything
//       Scryfall knows: name, cost, colours, types, power and toughness,
//       keywords, oracle text and rarity. The abilities are left empty, to
//       be written by hand in the effect format (docs/ARCHITECTURE.md §4).
//       An existing file is never overwritten.
//
//   node tools/scryfall.mjs check [--refresh]
//       Compares every card in data/cards with Scryfall and fails on any
//       difference in those fields. Rarity is not compared: Scryfall's is
//       per printing, and ours is the cube's own tier.
//
// Responses are cached in tools/.scryfall-cache (not in git), so a second
// run makes no requests; --refresh fetches again. Requests are spaced out as
// Scryfall's API guidelines ask.

import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const CARDS = join(ROOT, 'data', 'cards')
const CACHE = join(ROOT, 'tools', '.scryfall-cache')
const API = 'https://api.scryfall.com/cards/named?exact='
const HEADERS = { 'User-Agent': 'magic-roguelike-cardtool/0.1', Accept: 'application/json' }
const DELAY_MS = 600
const RETRIES = 5

const SUPERTYPES = ['Basic', 'Legendary', 'Snow', 'World']
const COLOR_ORDER = ['W', 'U', 'B', 'R', 'G']

/** Scryfall's names for the keyword abilities a card file lists. Its other
 * "keywords" (Enchant, Scry, Mill...) are actions or belong in abilities. */
const KEYWORDS = new Map(
  [
    'Deathtouch',
    'Defender',
    'Double strike',
    'First strike',
    'Flash',
    'Flying',
    'Haste',
    'Hexproof',
    'Indestructible',
    'Lifelink',
    'Menace',
    'Prowess',
    'Reach',
    'Trample',
    'Vigilance',
    'Ward',
  ].map((name) => [name, name.toLowerCase().replaceAll(' ', '_')]),
)

/** The id a card gets: its name in lower snake_case. */
export function cardId(name) {
  return name
    .toLowerCase()
    .replaceAll(/['’,]/g, '')
    .replaceAll(/[^a-z0-9]+/g, '_')
    .replaceAll(/^_+|_+$/g, '')
}

function sortColors(colors) {
  return [...colors].sort((a, b) => COLOR_ORDER.indexOf(a) - COLOR_ORDER.indexOf(b))
}

/** A whole number, or null for `*` and the like. */
function stat(value) {
  return value !== undefined && /^-?\d+$/.test(value) ? Number(value) : null
}

/**
 * The fields of a card file that come from Scryfall, in the file's order.
 * Throws for a card this game can't hold: one with two faces, or with a
 * power or toughness that isn't a number.
 */
export function fromScryfall(card) {
  if (card.card_faces) throw new Error(`${card.name} has more than one face`)
  const [left, right] = card.type_line.split(' — ')
  const words = left.split(' ')
  const supertypes = words.filter((word) => SUPERTYPES.includes(word))
  const types = words.filter((word) => !SUPERTYPES.includes(word))
  const fields = {
    id: cardId(card.name),
    name: card.name,
    mana_cost: card.mana_cost ?? '',
    colors: sortColors(card.colors ?? []),
    supertypes,
    types,
    subtypes: right ? right.split(' ') : [],
  }
  if (types.includes('Creature')) {
    fields.power = stat(card.power)
    fields.toughness = stat(card.toughness)
    if (fields.power === null || fields.toughness === null) {
      throw new Error(`${card.name} has power/toughness ${card.power}/${card.toughness}`)
    }
  }
  fields.rarity = card.rarity
  fields.keywords = (card.keywords ?? [])
    .filter((keyword) => KEYWORDS.has(keyword))
    .map((keyword) => KEYWORDS.get(keyword))
    .sort()
  fields.oracle_text = card.oracle_text ?? ''
  return fields
}

/** A new card file: the Scryfall fields, without the empty lists, and no
 * abilities yet. */
export function newCardFile(card) {
  const fields = fromScryfall(card)
  const file = {}
  for (const [key, value] of Object.entries(fields)) {
    if (Array.isArray(value) && value.length === 0) continue
    file[key] = value
  }
  file.abilities = []
  return file
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
const sorted = (list) => [...(list ?? [])].sort()

/** How a card file differs from Scryfall, one line per field. */
export function differences(file, card) {
  const expected = fromScryfall(card)
  const found = []
  const compare = (field, ours, theirs) => {
    if (!same(ours, theirs)) {
      found.push(`${field}: file has ${JSON.stringify(ours)}, Scryfall has ${JSON.stringify(theirs)}`)
    }
  }
  compare('id', file.id, expected.id)
  compare('name', file.name, expected.name)
  compare('mana_cost', file.mana_cost, expected.mana_cost)
  compare('colors', sorted(file.colors), sorted(expected.colors))
  compare('supertypes', sorted(file.supertypes), sorted(expected.supertypes))
  compare('types', sorted(file.types), sorted(expected.types))
  compare('subtypes', file.subtypes ?? [], expected.subtypes)
  compare('power', file.power ?? null, expected.power ?? null)
  compare('toughness', file.toughness ?? null, expected.toughness ?? null)
  compare('keywords', sorted(file.keywords), expected.keywords)
  compare('oracle_text', file.oracle_text ?? '', expected.oracle_text)
  return found
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

/** The Scryfall card with exactly this name, from the cache if it is there. */
async function fetchCard(name, refresh) {
  const cached = join(CACHE, `${cardId(name)}.json`)
  if (!refresh && existsSync(cached)) return JSON.parse(readFileSync(cached, 'utf8'))
  let response
  // Told to slow down, wait longer each time before asking again.
  for (let attempt = 1; attempt <= RETRIES; attempt += 1) {
    await sleep(DELAY_MS * attempt * attempt)
    response = await fetch(API + encodeURIComponent(name), { headers: HEADERS })
    if (response.status !== 429) break
  }
  if (response.status === 404) throw new Error(`Scryfall has no card named "${name}"`)
  if (!response.ok) throw new Error(`Scryfall answered ${response.status} for "${name}"`)
  const card = await response.json()
  mkdirSync(CACHE, { recursive: true })
  writeFileSync(cached, JSON.stringify(card))
  return card
}

async function add(names) {
  let failed = false
  for (const name of names) {
    try {
      const file = newCardFile(await fetchCard(name, false))
      const path = join(CARDS, `${file.id}.json`)
      if (existsSync(path)) {
        console.log(`${file.id}: already exists, left alone`)
        continue
      }
      writeFileSync(path, `${JSON.stringify(file, null, 2)}\n`)
      console.log(`${file.id}: written. ${file.mana_cost} ${file.types.join(' ')} (${file.rarity})`)
      console.log(`  ${file.oracle_text.replaceAll('\n', '\n  ')}`)
    } catch (error) {
      failed = true
      console.error(`${name}: ${error.message}`)
    }
  }
  return failed ? 1 : 0
}

async function check(refresh) {
  const files = readdirSync(CARDS).filter((name) => name.endsWith('.json')).sort()
  let wrong = 0
  for (const name of files) {
    const file = JSON.parse(readFileSync(join(CARDS, name), 'utf8'))
    try {
      const found = differences(file, await fetchCard(file.name, refresh))
      if (found.length === 0) continue
      wrong += 1
      console.error(`${name}:`)
      for (const line of found) console.error(`  ${line}`)
    } catch (error) {
      wrong += 1
      console.error(`${name}: ${error.message}`)
    }
  }
  console.log(`${files.length} cards checked against Scryfall, ${wrong} with differences.`)
  return wrong === 0 ? 0 : 1
}

async function main(args) {
  const [command, ...rest] = args
  if (command === 'add' && rest.length > 0) return add(rest)
  if (command === 'check') return check(rest.includes('--refresh'))
  console.error('Usage: node tools/scryfall.mjs add "Card Name" ...')
  console.error('       node tools/scryfall.mjs check [--refresh]')
  return 2
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  process.exitCode = await main(process.argv.slice(2))
}
