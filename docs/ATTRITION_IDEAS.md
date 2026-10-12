# Attrition: ideas, parked

Status: **picked up the same day; a first version is built.** Written on 2026-10-12 during the balance discussion for the Warden's first act as a list of ideas, not a recommendation. The owner then chose from it, and what was built is `DECISIONS.md` #68 and #69:

- **Feats**, from ideas 1 and 3: five are offered before a duel and two are picked, each in three tiers; their points set the card reward. They only ever add to a reward.
- **Wounds**, from ideas 4 and 9: a lost duel leaves an artifact that starts every duel on the player's battlefield and has to be paid off or endured. Wounds replace hearts, and the third ends the run. Boons are gone.

The rest of this file is as it was written, kept for the ideas not used yet: the judge (2), the boss that learns (5), feats that belong to the enemy (7), a tired commander (10).

## The problem

An act is, in practice, one duel long. In the sim nearly every deck reaches the boss (93 to 100% of runs, with 2.8 to 3.0 hearts of 3), so the act's win rate is the boss's win rate. Nothing a deck does in the first eleven rows wears it down.

That causes two things the owner dislikes:

- An easy duel has no tension, because nothing carries over (`DECISIONS.md` #66).
- The spread between a weak deck and a strong one has to come from a single game of Magic. The target is about 33% for a random pile and about 90% for an ideal deck. One duel gives that spread only with a very large difference in strength. Several duels that each cost a weak deck a little would give it from small differences, because the costs compound. That is how Slay the Spire gets its spread: a hallway fight rarely kills, it takes health.

So the game needs a way to say **how well** a duel was won, and to make a poor win cost something.

## What the owner has ruled out

- **Life carried over between duels as the standing rule.** Life and run health are separate (`DECISIONS.md` #7), so that lifegain and pay-life decks play as intended.
- **Win streaks.** A streak only counts wins and losses again.
- **Rewards for speed or for high life as the standing rule.** They favour some archetypes (`DECISIONS.md` #36).
- **Anything that changes how the player plays the duel.** Magic has many ways to win, and that is the fun of it.

The owner's own starting idea: different duels carry different conditions ("damage taken here carries over", "finish within N turns"), shown on the map, so the player picks the path that suits the deck. The worry is that this is limited. It appears below as idea 7, with two variations.

## How to judge an idea

| Question | Why it matters |
|---|---|
| Is it neutral between archetypes? | Aggro, lifegain, go wide and control should each be able to win "well". |
| Does it change how the duel is played? | Stalling to pad a score, or racing when the deck shouldn't, is the failure. |
| Can the player read it? | A cost the player can't predict feels like bad luck. |
| Does it wear a weak deck down more than a strong one? | That is the whole point. |
| What does it cost to build? | And does it need a card that isn't a Magic card? |

## The ideas

### A. Measure the win in several currencies, and accept any one

**1. Medals: win well in your own way.** Every duel offers the same few medals, each a different way of winning comfortably: *Swift* (won within N turns), *Unscathed* (ended at or above the starting life total), *Commanding* (ended with N or more creatures, or far more power on the battlefield), *Composed* (ended with N or more cards in hand). Earning **any one** means the win was clean. Earning none means it was a scrape, and a scrape costs something (see section D for what).
Each archetype has a natural medal, so nobody is asked to play differently, and a deck that is doing its own thing well gets one without trying. The risk is a player delaying a won game to collect one; thresholds that a healthy deck passes on the way to winning keep that small.

**2. The judge: how close did it get?** The engine already estimates positions for the AI. During the duel it records the player's worst moment: the lowest its evaluation of their position ever fell (or the lowest chance of winning its imagined games gave them). A duel where the player was never in danger is clean; one where they were nearly dead at some point is a scrape.
It can't be padded, because stalling never raises the low point, and it doesn't care how the win came. Its weakness is that the player can't see the formula, and that slow decks are "behind" early by design. It could be shown as a single gauge on the duel screen ("peril").

**3. Pick your own measure before the duel.** Before each duel the player is offered two of the measures from idea 1 and picks the one this duel will be judged on. It is the owner's idea moved from the path to the node: the choice is always available, not only where the map happens to offer it.

### B. Wear that doesn't depend on how the duel went

**4. Burdens.** After every duel won, the player takes one of two burdens for the rest of the act: the enemy starts with 2 more life, your first hand is a card smaller, enemies start with a 1/1, your commander costs {1} more. They are relics in reverse and use the existing modifier system. The act gets steadily harder, and the choice is a deck-building decision: take the burden your deck minds least. Combined with idea 1 or 2, a clean win could waive the burden.

**5. The boss learns.** The boss watches the run. Before the boss duel it gains one answer aimed at whatever the player's deck leaned on most across the act: most life gained, most tokens made, most counters placed, the commander cast most often. A deck with one trick meets a boss prepared for it; a deck with two packages gives it less to aim at. This rewards the bridge cards and is very Magic: the boss sideboards against you.

**6. Tired cards.** A card that has been cast in several duels is tired and sits the next one out, or comes back at a rest site. The player keeps a roster a little larger than the deck and rotates. It is neutral between archetypes, since every deck casts cards, and it makes depth matter instead of three good cards. It costs bookkeeping and a deck screen that shows it.

### C. Conditions on the duel itself

**7. Duel conditions on the map (the owner's idea).** Each duel node shows a condition: *Bloodied* (life you lose here is missing at the start of your next duel only), *Hunted* (win within N turns or the next enemy starts with an extra card), *Besieged* (the enemy starts with a blocker), and so on. The path is chosen to suit the deck. Carrying life for one duel under a named condition is different from the standing rule that was ruled out, but it does lean on it.

**8. Short skirmishes.** Some nodes are skirmishes at 10 life instead of full duels. With shorter fights an act can hold more of them, and more fights means the stronger deck wins out more reliably and each loss matters less. This connects to the parked idea of early enemies at 10 life.

### D. What a scrape or a loss costs

These are the currencies that ideas 1 to 3 could charge. They can also replace or sit beside hearts.

**9. Deck health: wounds.** A scrape, or a loss, adds a dead card to the deck; a rest site removes one. The deck's quality is its health, so nothing new has to be tracked on screen. The dead card can be a real one (a land that makes colourless mana, or a vanilla 0/1), which avoids the objection to "The Cursed Hoard" in `DECISIONS.md` #66.

**10. A tired commander.** The commander tax carries over: each time the commander is cast from the command zone, its cost for the rest of the act goes up a little, and a rest site resets it. A deck that wins without leaning on its commander pays nothing. This reuses a rule that exists and aims at the card a weak deck depends on most (in the sim, Jasmine Boreal is much of a random pile's strength).

**11. Fewer boons.** Boons are already bought with hearts. A scrape could cost half a heart, so that only clean acts arrive at the boss with every boon.

**12. Supplies.** The run has a small stock of second chances: a free mulligan, a redraw of a reward, an extra smoothing candidate. Scrapes use them up; clean wins don't.

## How they compare

| Idea | Neutral between archetypes | Leaves play alone | Readable | Bites weak decks more | Build cost |
|---|---|---|---|---|---|
| 1 Medals, any one | Yes, if each archetype has one | Mostly | Yes | Yes | Small |
| 2 The judge | Mostly (slow decks look worse early) | Yes | Poorly | Yes | Medium |
| 3 Pick your measure | Yes | Mostly | Yes | Yes | Small |
| 4 Burdens | Yes (the player chooses) | Yes | Yes | No, unless a clean win waives it | Small |
| 5 The boss learns | Yes | Yes | Yes, if shown | Bites narrow decks | Medium |
| 6 Tired cards | Yes | Yes | With a good deck screen | Bites shallow decks | Large |
| 7 Conditions on the map | No, by design | No, by design | Yes | Somewhat | Medium |
| 8 Skirmishes | Favours fast decks | Yes | Yes | Yes | Small |
| 9 Wounds | Yes | Yes | Yes | It is only the cost | Small |
| 10 Tired commander | Yes | Slightly | Yes | Yes | Small |

## A combination worth trying first

Medals (1) decide whether a win was clean; a scrape costs a wound (9) or a burden (4); a rest site heals. It is readable, uses the modifier system that exists, and each archetype has its own way to stay clean. The judge (2) is the more original measure and could replace the medals later if they turn out to distort play.

## How to test any of these

The sim can already compare a random pile with an ideal deck for each package. An attrition rule is working if it widens that gap without changing which packages are strongest. It also needs the owner's playtest: whether a scrape feels earned, and whether anyone plays a won duel differently to avoid one.
