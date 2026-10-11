import { type Page, expect, test } from '@playwright/test'

// Drives the real app in a browser. The clicking below is deliberately
// dumb: it only ever clicks what the screen lights up, which is how it
// checks that the screen always offers a way forward.

const idle = (page: Page) => page.locator('[data-testid="duel"][data-busy="false"]')
const primary = (page: Page) => page.locator('.prompt-bar button.primary')
const result = (page: Page) => page.getByTestId('result')

function watchForErrors(page: Page): string[] {
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(String(error)))
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  return errors
}

async function start(page: Page, query: string): Promise<void> {
  await page.goto(`/?${query}`)
  await page.getByRole('dialog', { name: 'Opening hand' }).waitFor()
  await idle(page).waitFor()
}

/** Takes one step of the game by clicking something the screen offers. */
async function clickSomething(page: Page, step: number): Promise<void> {
  const dialog = page.getByRole('dialog', { name: 'Opening hand' })
  const target = page.locator('.lit-target').first()
  const playable = page.locator('[data-testid="hand"] [data-playable="true"]').first()
  const attackAll = page.getByRole('button', { name: 'Attack with all' })
  const blocker = page.locator('.bf-row .permanent.lit-action').first()
  const reset = page.getByRole('button', { name: 'Reset', exact: true })

  if (await dialog.isVisible()) {
    const keep = dialog.getByRole('button', { name: 'Keep', exact: true })
    if (await keep.isVisible()) await keep.click()
    else await dialog.locator('.lit-action').first().click()
  } else if (await target.isVisible()) {
    await target.click()
  } else if ((await playable.isVisible()) && step % 4 !== 3) {
    await playable.click()
  } else if (await attackAll.isVisible()) {
    await attackAll.click()
    await primary(page).click()
  } else if ((await blocker.isVisible()) && step % 2 === 0) {
    await blocker.click()
  } else if (await primary(page).isVisible()) {
    await primary(page).click()
  } else if (await reset.isVisible()) {
    // Blocks that can't be confirmed as they stand.
    await reset.click()
  } else {
    // A decision answered from the hand, e.g. discarding.
    await page.locator('.lit-action').first().click()
  }
}

async function playToTheEnd(page: Page): Promise<number> {
  for (let step = 0; step < 1500; step += 1) {
    await idle(page).waitFor()
    if (await result(page).isVisible()) return step
    await expect(page.getByTestId('prompt').last()).not.toBeEmpty()
    await clickSomething(page, step)
  }
  throw new Error('the game did not end')
}

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

for (const [ai, seed] of [
  ['deck_a', 11],
  ['deck_c', 12],
] as const) {
  test(`a whole game against ${ai} can be clicked through`, async ({ page }) => {
    const errors = watchForErrors(page)
    await start(page, `seed=${seed}&you=deck_b&ai=${ai}&speed=instant`)
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
