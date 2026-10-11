import { type Page, expect } from '@playwright/test'

// Drives the real app in a browser. The clicking below is deliberately
// dumb: it only ever clicks what the screen lights up, which is how it
// checks that the screen always offers a way forward.

export const idle = (page: Page) => page.locator('[data-testid="duel"][data-busy="false"]')
export const primary = (page: Page) => page.locator('.prompt-bar button.primary')
export const result = (page: Page) => page.getByTestId('result')

export function watchForErrors(page: Page): string[] {
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(String(error)))
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  return errors
}

export async function start(page: Page, query: string): Promise<void> {
  await page.goto(`/?${query}`)
  await page.getByRole('dialog', { name: 'Opening hand' }).waitFor()
  await idle(page).waitFor()
}

/** Takes one step of the game by clicking something the screen offers. */
export async function clickSomething(page: Page, step: number): Promise<void> {
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
    // Blocks that can't be confirmed as they stand, e.g. one creature in
    // front of an attacker with menace. Start again and block nothing, or
    // the same block would be made and taken back for ever.
    await reset.click()
    await idle(page).waitFor()
    if (await primary(page).isVisible()) await primary(page).click()
  } else {
    // A decision answered from the hand, e.g. discarding.
    await page.locator('.lit-action').first().click()
  }
}

export async function playToTheEnd(page: Page): Promise<number> {
  for (let step = 0; step < 1500; step += 1) {
    await idle(page).waitFor()
    if (await result(page).isVisible()) return step
    await expect(page.getByTestId('prompt').last()).not.toBeEmpty()
    await clickSomething(page, step)
  }
  throw new Error('the game did not end')
}


/** An engine action, as far as following one by clicking needs to know it. */
interface Hint {
  type: string
  card?: number
  creature?: number
  blocker?: number
  attacker?: number
  source?: number
  sacrifice?: number | null
  targets?: ({ type: 'player'; player: string } | { type: 'object'; object: number })[]
}

/**
 * Takes one step of a run's duel by doing what the engine suggests, through
 * the screen: every click is on something a player could click. Needs
 * `hints` in the address.
 */
export async function followHint(page: Page): Promise<void> {
  const hint = await page.evaluate(() =>
    (window as unknown as { suggestDuelAction: () => unknown }).suggestDuelAction(),
  )
  const action = hint as Hint
  const permanent = (object: number | undefined) =>
    page.locator(`.permanent-slot[data-object="${object}"] > button.permanent`)
  const handCard = (object: number | undefined) =>
    page.locator(`.hand-card[data-object="${object}"]`)
  const button = (name: string) => page.getByRole('button', { name, exact: true })
  const clickTarget = async (target: NonNullable<Hint['targets']>[number]) => {
    if (target.type === 'player') {
      await page.locator(`[data-player="${target.player}"] button.life`).click()
    } else {
      // A creature, or a spell on the stack.
      await page
        .locator(
          `.permanent-slot[data-object="${target.object}"] > button.permanent, ` +
            `.stack-item[data-object="${target.object}"]`,
        )
        .click()
    }
  }

  switch (action.type) {
    case 'keep':
      await button('Keep').click()
      break
    case 'mulligan':
      await button('Mulligan').click()
      break
    case 'bottom_card':
    case 'discard':
    case 'play_land':
      await handCard(action.card).click()
      break
    case 'cast_spell':
      await handCard(action.card).click()
      for (const target of action.targets ?? []) await clickTarget(target)
      break
    case 'activate_ability':
      await permanent(action.source).click()
      if (action.sacrifice !== null && action.sacrifice !== undefined) {
        await permanent(action.sacrifice).click()
      }
      for (const target of action.targets ?? []) await clickTarget(target)
      break
    case 'choose_targets':
      for (const target of action.targets ?? []) await clickTarget(target)
      break
    case 'scry_keep':
      await button('Keep on top').click()
      break
    case 'scry_bottom':
      await button('Put on the bottom').click()
      break
    case 'attack':
    case 'remove_attacker':
      await permanent(action.creature).click()
      break
    case 'block':
      await permanent(action.blocker).click()
      await permanent(action.attacker).click()
      break
    case 'remove_blocker':
      await permanent(action.blocker).click()
      break
    case 'reset_declaration':
      await button('Reset').click()
      break
    case 'pass_priority':
    case 'confirm_attackers':
    case 'confirm_blockers':
    case 'confirm_damage':
      await primary(page).click()
      break
    default:
      throw new Error(`no way to click ${action.type}`)
  }
}
