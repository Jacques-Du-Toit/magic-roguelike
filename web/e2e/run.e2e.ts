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
    if (well) await followHint(page)
    else await clickSomething(page, step)
  }
  throw new Error('the duel did not end')
}

interface Seen {
  phases: Set<string>
  nodes: Set<string>
  duels: number
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
    case 'reward':
      await expect(page.getByTestId('reward-card')).toHaveCount(3)
      await page.getByTestId('reward-card').first().getByRole('button').click()
      return true
    case 'rest': {
      const heal = page.getByTestId('rest-heal')
      if (await heal.isEnabled()) {
        await heal.click()
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
    case 'boons': {
      const picks = Number(await page.getByTestId('picks-left').innerText())
      for (let pick = 0; pick < picks; pick += 1) {
        await page.locator('[data-testid="boon"]:enabled').first().click()
      }
      await expect(page.getByTestId('picks-left')).toHaveText('0')
      await expect(page.getByTestId('taken-boon')).toHaveCount(picks)
      await shot(page, 'run-boons-taken')
      await page.getByTestId('start-boss').click()
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
  const seen: Seen = { phases: new Set(), nodes: new Set(), duels: 0 }
  for (let step = 0; step < 200; step += 1) {
    await expect(run(page).or(duel(page))).toBeVisible()
    if (await duel(page).isVisible()) {
      seen.duels += 1
      if (seen.duels === 1) {
        await idle(page).waitFor()
        await shot(page, 'run-duel')
      }
      await playDuel(page, well)
    } else if (!(await runStep(page, seen))) {
      return seen
    }
  }
  throw new Error('the run did not end')
}

test('the menu starts a run with the chosen planeswalker', async ({ page }) => {
  const errors = watchForErrors(page)
  await page.goto('/')
  await expect(page.getByTestId('planeswalker-warden')).toBeChecked()
  await shot(page, 'menu')
  await page.getByTestId('planeswalker-firebrand').check()
  await page.getByTestId('run-seed-input').fill('nine')
  await page.getByTestId('start-run').click()
  await expect(page.getByRole('alert')).toHaveText('The seed must be a whole number.')
  await page.getByTestId('run-seed-input').fill('9')
  await page.getByTestId('start-run').click()

  await expect(run(page)).toHaveAttribute('data-phase', 'map')
  await expect(page.getByTestId('seed')).toHaveText('Seed 9')
  await expect(page.getByTestId('hearts')).toContainText('3/3 hearts')
  await expect(page.getByTestId('gold')).toHaveText('30 gold')
  await expect(page.getByTestId('passive')).toContainText('haste')
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

test('a duel in a run can be conceded, and costs a heart', async ({ page }) => {
  await page.goto('/?run=warden&seed=4&speed=instant')
  await page.locator('.map-node-open').first().click()
  await expect(duel(page)).toBeVisible()
  await expect(page.getByTestId('stakes')).toContainText('3 of 3 hearts')
  // The passive is on show in the duel.
  await expect(page.getByTestId('emblem')).toContainText("Warden's Welcome")
  await page.getByRole('button', { name: 'Concede' }).click()
  await page.getByTestId('concede').click()

  await expect(run(page)).toHaveAttribute('data-phase', 'map')
  await expect(page.getByTestId('hearts')).toContainText('2/3 hearts')
  await expect(page.getByTestId('news')).toContainText('You lost to')
  await expect(page.getByTestId('news')).toContainText('You lose 1 heart (2 left).')
  await expect(page.locator('.map-node-here')).toHaveCount(1)
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

test('a run played badly ends when the hearts run out', async ({ page }) => {
  test.setTimeout(600_000)
  const errors = watchForErrors(page)
  await page.goto('/?run=warden&seed=1&speed=instant')
  const seen = await playRun(page, false)
  await expect(page.getByTestId('run-result')).not.toBeEmpty()
  await expect(page.getByRole('alert')).toHaveCount(0)
  expect(seen.duels).toBeGreaterThanOrEqual(3)
  expect(errors).toEqual([])

  // The end of a run leads to another.
  await page.getByTestId('new-run').click()
  await expect(run(page)).toHaveAttribute('data-phase', 'map')
  await expect(page.getByTestId('seed')).not.toHaveText('Seed 1')
})

// Whole acts, played as well as the engine's own AI plays: every row of the
// map, the boons and the boss.
const ACTS: [string, number][] = process.env.ACTS
  ? process.env.ACTS.split(',').map((act) => {
      const [planeswalker, seed] = act.split(':')
      return [planeswalker, Number(seed)]
    })
  : [
      ['warden', 2],
      ['firebrand', 1],
    ]

for (const [planeswalker, seed] of ACTS) {
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
      `${planeswalker} seed ${seed}: ${seen.duels} duels, phases ${[...seen.phases].join(' ')}, ` +
        `nodes ${[...seen.nodes].join(' ')}, ${title}`,
    )
    await expect(page.getByTestId('run-result')).not.toBeEmpty()
    await expect(page.getByRole('alert')).toHaveCount(0)
    expect(errors).toEqual([])
    if (process.env.ACTS) return
    // These seeds lead through every kind of node. If a change to the game's
    // data moves them, ACTS=warden:1,warden:2,... finds seeds that do.
    expect([...seen.phases]).toEqual(
      expect.arrayContaining(['map', 'reward', 'shop', 'rest', 'boons', 'over']),
    )
    expect([...seen.nodes]).toEqual(expect.arrayContaining(['duel', 'elite', 'boss']))
  })
}