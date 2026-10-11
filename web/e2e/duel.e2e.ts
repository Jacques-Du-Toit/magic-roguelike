import { expect, test } from '@playwright/test'
import {
  clickSomething,
  idle,
  playToTheEnd,
  primary,
  result,
  start,
  watchForErrors,
} from './helpers.ts'

test('the menu starts a duel with the default matchup', async ({ page }) => {
  const errors = watchForErrors(page)
  await page.goto('/')
  await expect(page.getByTestId('human-deck')).toHaveValue('deck_b')
  await expect(page.getByTestId('ai-deck')).toHaveValue('random')
  await page.getByTestId('start').click()
  await expect(page.getByRole('dialog', { name: 'Opening hand' })).toBeVisible()
  await expect(page.getByTestId('seed')).toHaveText(/^Seed \d+$/)
  await expect(page.locator('[data-testid="hand"] .hand-card')).toHaveCount(7)
  expect(errors).toEqual([])
})

test('a bad seed is refused on the menu', async ({ page }) => {
  await page.goto('/')
  await page.getByTestId('seed-input').fill('seven')
  await page.getByTestId('start').click()
  await expect(page.getByRole('alert')).toHaveText('The seed must be a whole number.')
  await page.getByTestId('seed-input').fill('99999999999999999999999')
  await page.getByTestId('start').click()
  await expect(page.getByRole('alert')).toContainText('seed')
})

test('a mulligan draws a new hand and asks for a card to bottom', async ({ page }) => {
  await start(page, 'seed=7&you=deck_b&ai=deck_a&speed=instant')
  const dialog = page.getByRole('dialog', { name: 'Opening hand' })
  await dialog.getByRole('button', { name: 'Mulligan' }).click()
  await idle(page).waitFor()
  await expect(dialog.locator('.hand-card')).toHaveCount(7)
  await expect(dialog).toContainText('you 1')
  await dialog.getByRole('button', { name: 'Keep', exact: true }).click()
  await expect(page.getByTestId('prompt').first()).toContainText('Click 1 card')
  await dialog.locator('.hand-card.lit-action').first().click()
  await expect(dialog).toBeHidden()
  await expect(page.locator('[data-testid="hand"] .hand-card')).not.toHaveCount(7)
})

test('hovering a card enlarges it, and the graveyard can be viewed', async ({ page }) => {
  await start(page, 'seed=7&you=deck_b&ai=deck_a&speed=instant')
  await page.getByRole('button', { name: 'Keep', exact: true }).click()
  await idle(page).waitFor()
  const card = page.locator('[data-testid="hand"] .hand-card').first()
  await card.hover()
  const preview = page.getByTestId('card-preview')
  await expect(preview).toBeVisible()
  await expect(preview.locator('.card-name')).toHaveText(await card.locator('.card-name').innerText())
  await page.mouse.move(0, 0)
  await expect(preview).toBeHidden()

  await page.getByRole('button', { name: /^Graveyard/ }).last().click()
  const graveyard = page.getByRole('dialog', { name: 'Your graveyard' })
  await expect(graveyard).toContainText('No cards.')
  await graveyard.getByRole('button', { name: 'Close' }).click()
  await expect(graveyard).toBeHidden()
})

test('holding priority stops in steps that are otherwise passed through', async ({ page }) => {
  const query = 'seed=7&you=deck_b&ai=deck_a&speed=instant'
  const passFromFirstMainPhase = async (hold: boolean) => {
    await start(page, query)
    await page.getByRole('button', { name: 'Keep', exact: true }).click()
    await idle(page).waitFor()
    await expect(page.locator('.phase-current')).toHaveText('Main 1')
    if (hold) await page.getByLabel('Hold priority').check()
    await expect(primary(page)).toHaveText('Go to combat')
    await primary(page).click()
    await idle(page).waitFor()
  }

  await passFromFirstMainPhase(true)
  await expect(page.getByTestId('prompt')).toContainText('Beginning of combat')
  await expect(primary(page)).toHaveText('Pass')

  // Without it, a step with nothing to do goes by without a click.
  await passFromFirstMainPhase(false)
  await expect(page.getByTestId('prompt')).not.toContainText('Beginning of combat')
})

// The last two have the human on the drain deck, whose cards trigger on
// deaths, attacks and life gained, against the tempo deck's bounce and
// discard; on the token deck against the drain deck; and on the sacrifice
// deck, whose creatures have abilities to activate.
for (const [you, ai, seed] of [
  ['deck_b', 'deck_a', 11],
  ['deck_b', 'deck_c', 12],
  ['deck_e', 'deck_d', 13],
  ['deck_f', 'deck_e', 14],
  ['deck_g', 'deck_f', 15],
] as const) {
  test(`a whole game as ${you} against ${ai} can be clicked through`, async ({ page }) => {
    const errors = watchForErrors(page)
    await start(page, `seed=${seed}&you=${you}&ai=${ai}&speed=instant`)
    await playToTheEnd(page)
    await expect(result(page)).toHaveText(/You win|You lose|Draw/)
    await expect(page.getByRole('alert')).toHaveCount(0)
    await expect(page.getByTestId('log')).toContainText('the game')
    expect(errors).toEqual([])

    // The result screen leads to another game.
    await page.getByRole('button', { name: 'Play again' }).click()
    await expect(page.getByRole('dialog', { name: 'Opening hand' })).toBeVisible()
    await expect(page.getByTestId('seed')).not.toHaveText(`Seed ${seed}`)
  })
}

// The tempo deck is built from cards that stop for a choice: Opt scries,
// Man-o'-War's ability is aimed. TEMPO_SEED=n tries another game.
test('a game with the tempo deck shows a scry and the aiming of an ability', async ({ page }) => {
  const seed = process.env.TEMPO_SEED ?? '4'
  const errors = watchForErrors(page)
  await start(page, `seed=${seed}&you=deck_d&ai=deck_c&speed=instant`)
  const seen = new Set<string>()
  const shot = async (name: string) => {
    if (process.env.SHOTS && !seen.has(name)) {
      await page.screenshot({ path: `${process.env.SHOTS}/duel-${name}.png` })
    }
    seen.add(name)
  }
  for (let step = 0; step < 1500; step += 1) {
    await idle(page).waitFor()
    if (await result(page).isVisible()) break
    const prompt = await page.getByTestId('prompt').last().innerText()
    if (await page.getByTestId('scry-panel').isVisible()) {
      await expect(page.getByTestId('scry-panel').locator('.card')).toHaveCount(1)
      await expect(page.getByRole('button', { name: 'Keep on top' })).toBeVisible()
      await expect(page.getByRole('button', { name: 'Put on the bottom' })).toBeVisible()
      await shot('scry')
      // Bottom the first one seen, to use both buttons in one game.
      if (!seen.has('bottomed')) {
        seen.add('bottomed')
        await page.getByRole('button', { name: 'Put on the bottom' }).click()
        continue
      }
    }
    // (Casting a spell with a target says "..., or cancel." instead.)
    if (prompt.startsWith('Choose a target for') && prompt.endsWith('ability.')) {
      expect(await page.locator('.lit-target').count()).toBeGreaterThan(1)
      await shot('aim')
    }
    await clickSomething(page, step)
  }
  await expect(result(page)).toHaveText(/You win|You lose|Draw/)
  await expect(page.getByRole('alert')).toHaveCount(0)
  expect(errors).toEqual([])
  console.log(`tempo deck, seed ${seed}: saw ${[...seen].join(' ')}`)
  if (process.env.TEMPO_SEED) return
  expect([...seen]).toEqual(expect.arrayContaining(['scry', 'aim']))
})

// The sacrifice deck's creatures have abilities to activate, some of which
// ask for a creature to give up. SACRIFICE_SEED=n tries another game.
test('a game with the sacrifice deck shows an ability being used and a sacrifice chosen', async ({
  page,
}) => {
  const seed = process.env.SACRIFICE_SEED ?? '2'
  const errors = watchForErrors(page)
  await start(page, `seed=${seed}&you=deck_g&ai=deck_c&speed=instant`)
  const seen = new Set<string>()
  for (let step = 0; step < 1500; step += 1) {
    await idle(page).waitFor()
    if (await result(page).isVisible()) break
    const prompt = await page.getByTestId('prompt').last().innerText()
    if (prompt.startsWith('Choose a creature to sacrifice')) {
      expect(await page.locator('.lit-target').count()).toBeGreaterThan(0)
      if (process.env.SHOTS && !seen.has('sacrifice')) {
        await page.screenshot({ path: `${process.env.SHOTS}/duel-sacrifice.png` })
      }
      seen.add('sacrifice')
    }
    await clickSomething(page, step)
  }
  await expect(result(page)).toHaveText(/You win|You lose|Draw/)
  await expect(page.getByRole('alert')).toHaveCount(0)
  expect(errors).toEqual([])
  const log = await page.getByTestId('log').innerText()
  if (/You use .* ability/.test(log)) seen.add('used')
  if (log.includes('You sacrifice')) seen.add('sacrificed')
  console.log(`sacrifice deck, seed ${seed}: saw ${[...seen].join(' ')}`)
  if (process.env.SACRIFICE_SEED) return
  expect([...seen]).toEqual(expect.arrayContaining(['sacrifice', 'used', 'sacrificed']))
})

test('the same seed can be replayed, and events play with their pauses', async ({ page }) => {
  await start(page, 'seed=21&you=deck_b&ai=deck_c&speed=fast')
  const firstHand = await page.locator('[data-testid="hand"] .card-name').allInnerTexts()
  for (let step = 0; step < 12; step += 1) {
    await idle(page).waitFor()
    await clickSomething(page, step)
  }
  await idle(page).waitFor()
  await expect(page.getByTestId('turn')).toHaveText(/^Turn \d+/)

  await page.getByRole('button', { name: 'Quit' }).click()
  await expect(page.getByTestId('seed-input')).toHaveValue('21')
  await page.getByTestId('start').click()
  await idle(page).waitFor()
  expect(await page.locator('[data-testid="hand"] .card-name').allInnerTexts()).toEqual(firstHand)
})
