// Run with: node --test tools/

import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cardId, differences, fromScryfall, newCardFile } from './scryfall.mjs'

const overseer = {
  name: 'Inspiring Overseer',
  mana_cost: '{2}{W}',
  colors: ['W'],
  type_line: 'Creature — Angel Cleric',
  power: '2',
  toughness: '1',
  rarity: 'common',
  keywords: ['Flying'],
  oracle_text: 'Flying\nWhen this creature enters, you gain 1 life and draw a card.',
}

const forest = {
  name: 'Forest',
  mana_cost: '',
  colors: [],
  type_line: 'Basic Land — Forest',
  rarity: 'common',
  keywords: [],
  oracle_text: '({T}: Add {G}.)',
}

test('ids are the name in lower snake_case', () => {
  assert.equal(cardId("Healer's Hawk"), 'healers_hawk')
  assert.equal(cardId("Man-o'-War"), 'man_o_war')
  assert.equal(cardId('Vampire of the Dire Moon'), 'vampire_of_the_dire_moon')
  assert.equal(cardId('Borborygmos, Enraged'), 'borborygmos_enraged')
})

test('a creature is read with its types, stats and keywords', () => {
  assert.deepEqual(fromScryfall(overseer), {
    id: 'inspiring_overseer',
    name: 'Inspiring Overseer',
    mana_cost: '{2}{W}',
    colors: ['W'],
    supertypes: [],
    types: ['Creature'],
    subtypes: ['Angel', 'Cleric'],
    power: 2,
    toughness: 1,
    rarity: 'common',
    keywords: ['flying'],
    oracle_text: overseer.oracle_text,
  })
})

test('supertypes are split off, colours are in WUBRG order, actions are not keywords', () => {
  const card = fromScryfall({
    ...overseer,
    name: 'Some Legend',
    colors: ['G', 'W'],
    type_line: 'Legendary Creature — Elf',
    keywords: ['Scry', 'First strike', 'Enchant', 'Double strike'],
  })
  assert.deepEqual(card.supertypes, ['Legendary'])
  assert.deepEqual(card.types, ['Creature'])
  assert.deepEqual(card.colors, ['W', 'G'])
  assert.deepEqual(card.keywords, ['double_strike', 'first_strike'])
})

test('a new card file leaves out empty lists and has no abilities yet', () => {
  assert.deepEqual(newCardFile(forest), {
    id: 'forest',
    name: 'Forest',
    mana_cost: '',
    supertypes: ['Basic'],
    types: ['Land'],
    subtypes: ['Forest'],
    rarity: 'common',
    oracle_text: '({T}: Add {G}.)',
    abilities: [],
  })
})

test('cards this game cannot hold are refused', () => {
  assert.throws(() => fromScryfall({ ...overseer, card_faces: [{}, {}] }), /more than one face/)
  assert.throws(() => fromScryfall({ ...overseer, power: '*' }), /power\/toughness/)
})

test('a matching file has no differences, whatever its rarity or abilities', () => {
  const file = { ...newCardFile(overseer), rarity: 'rare', abilities: [{ kind: 'static' }] }
  assert.deepEqual(differences(file, overseer), [])
  assert.deepEqual(differences({ ...file, colors: ['W'], keywords: ['flying'] }, overseer), [])
})

test('an altered card keeps its own text, and everything else is still checked', () => {
  const file = { ...newCardFile(overseer), oracle_text: 'Our own wording.' }
  assert.equal(differences(file, overseer).length, 1)
  assert.deepEqual(differences({ ...file, altered: true }, overseer), [])
  assert.match(differences({ ...file, altered: true, toughness: 9 }, overseer)[0], /^toughness/)
})

test('each field that differs is reported', () => {
  const file = { ...newCardFile(overseer), mana_cost: '{1}{W}', toughness: 2, keywords: [] }
  const found = differences(file, overseer)
  assert.equal(found.length, 3)
  assert.match(found[0], /^mana_cost: file has "\{1\}\{W\}", Scryfall has "\{2\}\{W\}"$/)
  assert.match(found[1], /^toughness/)
  assert.match(found[2], /^keywords/)
})
