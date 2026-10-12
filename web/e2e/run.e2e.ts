import { type Page, expect, test } from '@playwright/test'
import { clickSomething, followHint, idle, result, watchForErrors } from './helpers.ts'

// Clicks through runs in the real app. Like the duel tests, it only ever
// clicks what the screen offers.

const run = (page: Page) => page.getByTestId('run')
const duel = (page: Page) => page.getByTestId('duel')

/** Saves a picture of the screen when SHOTS names a folder. */
async function shot(page: Page, name: string): Promise<void> {
  const folder = process.env.SHOTS
  if (folder) await page.screenshot({ path: `${folder}/${name}.png` })
}

/**
 * Plays the duel on screen to its result and goes back to the run. With
 * `well`, it makes the moves the engine suggests; otherwise it clicks
 * whatever the screen lights up.
 */
async function playDuel(page: Page, well: boolean): Promise<void> {
  for (let step = 0; step < 1500; step += 1) {
    await idle(page).waitFor()
    if (await result(page).isVisible()) {
      await page.getByTestId('continue').click()
      return
    }
    await expect(page.getByTestId('prompt').last()).not.toBeEmpty()
    if (await page.getByTestId('damage-panel').isVisible()) {
      await shot(page, 'duel-damage-division')
      console.log('damage division seen')
    }
    if (well) await followHint(page)
    else await clickSomething(page, step)
  }
  throw new Error('the duel did not end')
}

/**
 * Before a duel: ticks the first two feats on offer and starts the duel.
 * The button only works once two are ticked.
 */
async function pickFeats(page: Page): Promise<void> {
  await expect(run(page)).toHaveAttribute('data-phase', 'feats')
  await expect(page.getByTestId('feats-enemy')).not.toBeEmpty()
  const start = page.getByTestId('start-duel')
  await expect(start).toBeDisabled()
  await page.getByTestId('feat').nth(0).click()
  await expect(start).toHaveText('Choose 1 more')
  await page.getByTestId('feat').nth(1).click()
  await expect(page.locator('[data-testid="feat"][aria-pressed="true"]')).toHaveCount(2)
  await start.click()
  await expect(duel(page)).toBeVisible()
}

interface Seen {
  phases: Set<string>
  nodes: Set<string>
  duels: number
  /** Duels that showed the two feats picked for them. */
  duelsWithFeats: number
  /** Whether a wound was ever carried. */
  wounded: boolean
  /** Duels in which the enemy showed a commander. */
  enemyCommanders: number
  /** The names of the events met. */
  events: string[]
}

/** Does one thing on the run screen. Returns false once the run is over. */
async function runStep(page: Page, seen: Seen): Promise<boolean> {
  const phase = (await run(page).getAttribute('data-phase')) ?? ''
  if (!seen.phases.has(phase)) await shot(page, `run-${phase}`)
  seen.phases.add(phase)
  await expect(page.getByTestId('run-title')).not.toBeEmpty()
  switch (phase) {
    case 'map': {
      const open = page.locator('.map-node-open')
      expect(await open.count()).toBeGreaterThan(0)
      // Something new, if there is any.
      const kinds = await open.evaluateAll((nodes) =>
        nodes.map((node) => node.getAttribute('data-kind') ?? ''),
      )
      const fresh = kinds.findIndex((kind) => !seen.nodes.has(kind))
      const pick = fresh === -1 ? 0 : fresh
      seen.nodes.add(kinds[pick])
      await open.nth(pick).click()
      return true
    }
    case 'feats': {
      // Five of the Warden's nine, or of the six shared ones for the Firebrand.
      await expect(page.getByTestId('feat')).toHaveCount(5)
      await expect(page.getByTestId('feat-rewards').locator('li')).toHaveCount(4)
      await pickFeats(page)
      return true
    }
    case 'reward': {
      // Two to four cards: by how the feats went, or two after a loss.
      const cards = await page.getByTestId('reward-card').count()
      expect(cards).toBeGreaterThanOrEqual(2)
      expect(cards).toBeLessThanOrEqual(4)
      await page.getByTestId('reward-card').first().getByRole('button').click()
      return true
    }
    case 'draft': {
      // Five cards, of which two are kept; the pack shrinks as they go.
      const deck = page.getByTestId('open-deck')
      const before = await deck.innerText()
      await expect(page.getByTestId('draft-card')).toHaveCount(5)
      await expect(page.getByTestId('draft-picks')).toHaveText('2')
      await page.getByTestId('draft-card').first().getByRole('button').click()
      await expect(page.getByTestId('draft-card')).toHaveCount(4)
      await expect(page.getByTestId('draft-picks')).toHaveText('1')
      await page.getByTestId('draft-card').last().getByRole('button').click()
      await expect(run(page)).toHaveAttribute('data-phase', 'map')
      await expect(deck).not.toHaveText(before)
      return true
    }
    case 'treasure': {
      // Two relics or the gold: a relic taken joins the side panel.
      const owned = await page.getByTestId('relic').count()
      await expect(page.getByTestId('treasure-relic')).toHaveCount(2)
      await expect(page.getByTestId('take-gold')).toContainText('40 gold instead')
      const name = await page.getByTestId('treasure-relic').first().locator('strong').innerText()
      await page.getByTestId('treasure-relic').first().getByRole('button').click()
      await expect(run(page)).toHaveAttribute('data-phase', 'map')
      await expect(page.getByTestId('relic')).toHaveCount(owned + 1)
      await expect(page.getByTestId('relic').last()).toContainText(name)
      await expect(page.getByTestId('news')).toContainText(`You gain a relic: ${name}.`)
      return true
    }
    case 'event': {
      // The first thing the event offers that can be had; if there is
      // nothing, the way on.
      const name = await page.getByTestId('run-title').innerText()
      seen.events.push(name)
      await expect(page.getByTestId('event-text')).not.toBeEmpty()
      await expect(page.getByTestId('leave')).toBeEnabled()
      const option = page.locator('[data-testid="event-option"]:enabled').first()
      if (!(await option.isVisible())) {
        await page.getByTestId('leave').click()
        await expect(run(page)).toHaveAttribute('data-phase', 'map')
        return true
      }
      const relics = await page.getByTestId('relic').count()
      const wounds = await page.getByTestId('wounds').innerText()
      await option.click()
      if (name === 'The Wandering Smith') {
        // A card of the deck is given up for another.
        const deck = page.getByRole('dialog', { name: 'Choose a card' })
        await deck.getByRole('button').first().click()
        await expect(deck).toBeHidden()
        await expect(page.getByTestId('news')).toContainText('leaves your deck.')
        await expect(page.getByTestId('news')).toContainText('joins your deck.')
      }
      if (name === "The Devil's Bargain") {
        // One of the two relics, for a wound.
        await expect(page.getByTestId('relic')).toHaveCount(relics + 1)
        await expect(page.getByTestId('wounds')).not.toHaveText(wounds)
        await expect(page.getByTestId('news')).toContainText('You take a wound: Festering Wound.')
      }
      await expect(run(page)).toHaveAttribute('data-phase', 'map')
      await expect(page.getByTestId('news')).not.toBeEmpty()
      return true
    }
    case 'rest': {
      // A wound is healed if there is one: the side panel loses it.
      const heal = page.getByTestId('rest-heal').first()
      if (await heal.isEnabled()) {
        const carried = await page.getByTestId('wound').count()
        await heal.click()
        await expect(page.getByTestId('wound')).toHaveCount(carried - 1)
        await expect(page.getByTestId('news')).toContainText('is healed.')
      } else {
        await page.getByTestId('rest-remove').click()
        const deck = page.getByRole('dialog', { name: 'Remove a card' })
        await deck.getByRole('button').first().click()
        await expect(deck).toBeHidden()
      }
      return true
    }
    case 'shop': {
      const buy = page.locator('[data-testid="buy"]:enabled').first()
      if (await buy.isVisible()) {
        const gold = await page.getByTestId('gold').innerText()
        await buy.click()
        await expect(page.getByTestId('gold')).not.toHaveText(gold)
      }
      await page.getByTestId('leave').click()
      return true
    }
    case 'over':
      return false
    default:
      throw new Error(`unknown phase ${phase}`)
  }
}

/** Clicks through the run on screen until it is over. */
async function playRun(page: Page, well: boolean): Promise<Seen> {
  const seen: Seen = {
    phases: new Set(),
    nodes: new Set(),
    duels: 0,
    duelsWithFeats: 0,
    wounded: false,
    enemyCommanders: 0,
    events: [],
  }
  for (let step = 0; step < 200; step += 1) {
    await expect(run(page).or(duel(page))).toBeVisible()
    if (await duel(page).isVisible()) {
      seen.duels += 1
      if (seen.duels === 1) {
        await idle(page).waitFor()
        await shot(page, 'run-duel')
      }
      // The two feats picked are on show, with what they would score now;
      // the boss has none.
      const feats = await page.getByTestId('duel-feat').count()
      expect([0, 2]).toContain(feats)
      if (feats === 2) seen.duelsWithFeats += 1
      // A wound carried is on the battlefield from the start.
      if (await page.locator('.permanent-slot[data-card="festering_wound"]').first().isVisible()) {
        if (!seen.wounded) await shot(page, 'run-duel-wounded')
        seen.wounded = true
      }
      // The player always has a commander; elites and the boss show theirs.
      await expect(page.locator('[data-player="P1"] [data-testid="commander"]')).toBeVisible()
      if (await page.locator('[data-player="P2"] [data-testid="commander"]').isVisible()) {
        seen.enemyCommanders += 1
        if (seen.enemyCommanders === 1) {
          await idle(page).waitFor()
          await shot(page, 'run-duel-enemy-commander')
        }
      }
      await playDuel(page, well)
    } else if (!(await runStep(page, seen))) {
      return seen
    }
  }
  throw new Error('the run did not end')
}

test('the menu starts a run with the chosen commander and emblem', async ({ page }) => {
  const errors = watchForErrors(page)
  await page.goto('/')
  await expect(page.getByTestId('planeswalker-warden')).toBeChecked()
  await shot(page, 'menu')
  // Each character shows its commander, and comes with its usual emblem.
  await expect(page.getByTestId('menu-commander').first()).toContainText('Jasmine Boreal')
  await expect(page.getByTestId('emblem-emblem_welcome')).toBeChecked()
  await page.getByTestId('planeswalker-firebrand').check()
  await expect(page.getByTestId('emblem-emblem_fury')).toBeChecked()
  // Any emblem goes with any commander.
  await page.getByTestId('emblem-emblem_welcome').check()
  await shot(page, 'menu-firebrand-welcome')
  await page.getByTestId('run-seed-input').fill('nine')
  await page.getByTestId('start-run').click()
  await expect(page.getByRole('alert')).toHaveText('The seed must be a whole number.')
  await page.getByTestId('run-seed-input').fill('9')
  await page.getByTestId('start-run').click()

  await expect(run(page)).toHaveAttribute('data-phase', 'map')
  await expect(page.getByTestId('seed')).toHaveText('Seed 9')
  await expect(page.getByTestId('wounds')).toContainText('0/3 wounds')
  await expect(page.getByTestId('gold')).toHaveText('30 gold')
  await expect(page.getByTestId('starting-emblem')).toContainText('Warm Welcome')
  await expect(page.getByTestId('run-commander')).toContainText('Boris Devilboon')
  // Three ways to start, and the boss at the far end.
  await expect(page.locator('.map-node-open')).toHaveCount(3)
  await expect(page.locator('.map-node[data-kind="boss"]')).toHaveCount(1)

  // The deck: 15 spells and the 10 basic lands that go with them.
  await page.getByTestId('open-deck').click()
  const deck = page.getByRole('dialog', { name: 'Your deck' })
  await expect(deck.locator('.run-card')).toHaveCount(15)
  await expect(deck).toContainText('25 cards')
  await expect(deck).toContainText('Mountain')
  await shot(page, 'run-deck')
  await page.getByTestId('close-deck').click()
  await expect(deck).toBeHidden()
  expect(errors).toEqual([])
})

test('feats are picked before a duel, and a duel conceded leaves a wound', async ({ page }) => {
  const errors = watchForErrors(page)
  await page.goto('/?run=warden&seed=4&speed=instant')
  await page.locator('.map-node-open').first().click()

  // Five feats, each with its three tiers, and what the points pay.
  await expect(run(page)).toHaveAttribute('data-phase', 'feats')
  await expect(page.getByTestId('run-title')).toHaveText('Before the duel: choose 2 feats')
  await expect(page.getByTestId('feat')).toHaveCount(5)
  await expect(page.getByTestId('feat').first()).toContainText('/')
  await expect(page.getByTestId('feat-rewards')).toContainText('0 points: 2 cards to choose from')
  await expect(page.getByTestId('feat-rewards')).toContainText('5 or more points: 4 cards')
  // A third tick lets go of the first.
  for (const place of [4, 3, 2]) await page.getByTestId('feat').nth(place).click()
  await expect(page.getByTestId('feat').nth(4)).toHaveAttribute('aria-pressed', 'false')
  await expect(page.getByTestId('start-duel')).toBeEnabled()
  // And a tick can be taken back.
  for (const place of [3, 2]) await page.getByTestId('feat').nth(place).click()
  await shot(page, 'run-feats')
  const names = await page.getByTestId('feat').locator('strong').allInnerTexts()
  await pickFeats(page)

  // The two picked are on show in the duel, with nothing scored for sure yet.
  await expect(page.getByTestId('duel-feat')).toHaveCount(2)
  await expect(page.getByTestId('duel-feat').nth(0)).toContainText(names[0])
  await expect(page.getByTestId('duel-feat').nth(1)).toContainText(names[1])
  await expect(page.getByTestId('stakes')).toContainText('0 of 3 wounds')
  await expect(page.getByTestId('stakes')).toContainText('Losing this duel leaves a wound.')
  // The starting emblem is on show in the duel.
  await expect(page.getByTestId('emblem')).toContainText('Warm Welcome')
  await page.getByRole('button', { name: 'Concede' }).click()
  await page.getByTestId('concede').click()

  // A wound, and a smaller reward than a win's.
  await expect(run(page)).toHaveAttribute('data-phase', 'reward')
  await expect(page.getByTestId('wounds')).toContainText('1/3 wounds')
  await expect(page.getByTestId('wound')).toContainText('Festering Wound')
  await expect(page.getByTestId('news')).toContainText('You lost to')
  await expect(page.getByTestId('news')).toContainText('You take a wound: Festering Wound.')
  await expect(page.getByTestId('reward-card')).toHaveCount(2)
  await shot(page, 'run-wounded')
  await page.getByTestId('skip').click()
  await expect(run(page)).toHaveAttribute('data-phase', 'map')
  await expect(page.locator('.map-node-here')).toHaveCount(1)

  // The wound is on the battlefield in the next duel, and can be paid off.
  await page.locator('.map-node-open').first().click()
  await pickFeats(page)
  await idle(page).waitFor()
  await expect(page.locator('.permanent-slot[data-card="festering_wound"]')).toHaveCount(1)
  await expect(page.getByTestId('stakes')).toContainText('1 of 3 wounds')
  await page.getByRole('button', { name: 'Keep', exact: true }).click()
  await idle(page).waitFor()
  await shot(page, 'run-duel-wound')
  expect(errors).toEqual([])
})

test('the commander waits in the command zone and is cast from it', async ({ page }) => {
  test.setTimeout(300_000)
  const errors = watchForErrors(page)
  await page.goto('/?run=warden&seed=2&speed=instant&hints=1')
  await page.locator('.map-node-open').first().click()
  await pickFeats(page)
  await idle(page).waitFor()
  const mine = page.locator('[data-player="P1"] [data-testid="commander"]')
  await expect(mine).toContainText('Jasmine Boreal')
  // An easy enemy has none.
  await expect(page.locator('[data-player="P2"] [data-testid="commander"]')).toHaveCount(0)

  // Play as the engine suggests until the commander is on the battlefield.
  const zone = page.getByTestId('command-zone')
  const badge = page.getByTestId('commander-badge')
  let sawZone = false
  for (let step = 0; step < 800; step += 1) {
    await idle(page).waitFor()
    if (await badge.isVisible()) break
    expect(await result(page).isVisible(), 'the duel ended before the commander was cast').toBe(false)
    if (!sawZone && (await zone.isVisible())) {
      sawZone = true
      await expect(zone).toContainText('Commander')
      await expect(zone.locator('.hand-card')).toHaveAttribute('data-card', 'jasmine_boreal')
      await shot(page, 'duel-command-zone')
    }
    await followHint(page)
  }
  expect(sawZone).toBe(true)
  await expect(badge).toBeVisible()
  await expect(zone).toHaveCount(0)
  await expect(mine).toContainText('in play')
  await expect(page.getByTestId('log')).toContainText('You cast Jasmine Boreal')
  await shot(page, 'duel-commander-in-play')
  expect(errors).toEqual([])
})

test('the same seed gives the same run', async ({ page }) => {
  const kinds = async () => {
    await page.goto('/?run=firebrand&seed=12&speed=instant')
    await expect(run(page)).toBeVisible()
    return page
      .locator('.map-node')
      .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-kind')))
  }
  const first = await kinds()
  expect(first.length).toBeGreaterThan(8)
  expect(await kinds()).toEqual(first)
})

test('a run played badly ends with one wound too many', async ({ page }) => {
  test.setTimeout(600_000)
  const errors = watchForErrors(page)
  await page.goto('/?run=warden&seed=1&speed=instant')
  const seen = await playRun(page, false)
  await expect(page.getByTestId('run-result')).not.toBeEmpty()
  await expect(page.getByRole('alert')).toHaveCount(0)
  expect(seen.duels).toBeGreaterThanOrEqual(3)
  // Wounds were carried into later duels, where they sat on the battlefield.
  expect(seen.wounded).toBe(true)
  expect(seen.duelsWithFeats).toBeGreaterThanOrEqual(3)
  expect(errors).toEqual([])

  // The end of a run leads to another.
  await page.getByTestId('new-run').click()
  await expect(run(page)).toHaveAttribute('data-phase', 'map')
  await expect(page.getByTestId('seed')).not.toHaveText('Seed 1')
})

// Whole acts, played as well as the engine's own AI plays: every row of the
// map, the feats before each duel, and the boss.
// Each seed leads through every kind of node, and to one of the two events
// that have a screen of their own: for choosing a card, or a relic.
const ACTS: [string, number, string?][] = process.env.ACTS
  ? process.env.ACTS.split(',').map((act) => {
      const [planeswalker, seed] = act.split(':')
      return [planeswalker, Number(seed)]
    })
  : [
      ['warden', 17, "The Devil's Bargain"],
      ['firebrand', 26, 'The Wandering Smith'],
    ]

for (const [planeswalker, seed, event] of ACTS) {
  test(`a whole act as ${planeswalker} (seed ${seed}) can be clicked through to the boss`, async ({
    page,
  }) => {
    test.setTimeout(600_000)
    const errors = watchForErrors(page)
    await page.goto(`/?run=${planeswalker}&seed=${seed}&speed=instant&hints=1`)
    const seen = await playRun(page, true)
    await shot(page, `run-over-${planeswalker}`)
    const title = await page.getByTestId('run-title').innerText()
    console.log(
      `${planeswalker} seed ${seed}: ${seen.duels} duels${seen.wounded ? ' (wounded)' : ''}, ` +
        `phases ${[...seen.phases].join(' ')}, ` +
        `nodes ${[...seen.nodes].join(' ')}, events ${seen.events.join(', ') || 'none'}, ${title}`,
    )
    await expect(page.getByTestId('run-result')).not.toBeEmpty()
    await expect(page.getByRole('alert')).toHaveCount(0)
    expect(errors).toEqual([])
    if (process.env.ACTS) return
    // If a change to the game's data moves these seeds,
    // ACTS=warden:1,warden:2,... prints what other seeds lead through.
    expect([...seen.phases]).toEqual(
      expect.arrayContaining([
        'map',
        'feats',
        'reward',
        'draft',
        'treasure',
        'event',
        'shop',
        'rest',
        'over',
      ]),
    )
    // Every duel but the boss's had its two feats.
    expect(seen.duelsWithFeats).toBe(seen.duels - 1)
    if (event) expect(seen.events).toContain(event)
    expect([...seen.nodes]).toEqual(expect.arrayContaining(['duel', 'elite', 'boss']))
    // The elite and the boss each brought a commander.
    expect(seen.enemyCommanders).toBeGreaterThanOrEqual(2)
  })
}