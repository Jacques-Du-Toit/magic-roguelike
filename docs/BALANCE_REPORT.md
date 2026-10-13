# Balance report: the Warden's first act

One overnight session (2026-10-13) on the branch `balance-lab`. Everything here is a proposal: `DECISIONS.md` and `ROADMAP.md` are untouched, and nothing is on `main`.

## 1. In short

The Warden starts with a weaker deck and no emblem. The pool is 104 cards in five packages, with the cards that are good in any deck held back. The enemies are rebuilt in five tiers, the late ones from decks that grow as the game goes on, and the boss has 50 life. Against that field a random pile wins about a third of acts and a deck that commits to a package wins four in five or better.

| Criterion | Target | Measured | Met? |
|---|---|---|---|
| 1. Floor: the random picker, with relics | about 33% | **34.2%** (600 runs, +/- 3.8) | Yes |
| 2. Ceiling: each package's ideal deck, no relics | 85 to 95%, within about 10 points | wide **89.3**, lifegain **86.0**, big **82.7**, auras **80.3**, counters **77.7** (300 runs each, +/- 3.5 to 4.7) | Partly: two in the band, three 2 to 7 points under it; the spread is 11.6 |
| 3. The gap comes from the whole act | through wounds and feats | The random picker takes 1.67 wounds a run, dies before the boss in 17% of runs and beats it 41% of the time. Ideal decks take 0.6 to 0.9, die before it in 1 to 7% and beat it 83 to 91% | Partly: most of the gap is still made in the boss duel |
| 4. No relic, common or uncommon worth more than about 5 points | 5 | All relics together are worth 7 points of act win rate (34.2 against 27.0). Card by card, see section 3 | See section 3 |
| 5. Strong and versatile cards are out | | A deck of the best 18 cards of every package wins 58% of single duels where the package decks win 81 to 86% | Yes |
| 6. Tiers | | Applied; four judgement calls listed | Mostly |
| 7. Relics small, conditional, glue, in tiers | | 13 offered, 10 of them tied to a package and 6 of those to a pair; two strong ones held back; tiers proposed, not built | Partly |
| 8 to 12. Fun | | Not measurable by the sim; what was done for each is in section 3 | For you to play |

What is plainly **not met**: three of the five ceilings; the 10-point spread (by 1.6); relic tiers as a mechanism. What I could not judge at all: the fun criteria.

Two things to know before reading further:

- **The AI is the measuring instrument, and it plays slow engines badly.** It looks one turn cycle ahead and values an enchantment on the battlefield at a flat number, so cards that pay off over several turns (Cathars' Crusade, Hardened Scales, Collective Unconscious, Sigil of the Empty Throne) measure as worth less than a Grizzly Bears. Every ceiling here is the AI's ceiling. A person will get more from those cards, so the real ceilings are higher than measured and the engines may be stronger than they look.
- **The Firebrand is broken on this branch**: 2.7% of acts won (300 runs). Its emblem is gone and the field and the boss were tuned against the Warden's pool. Only the Warden was in scope.

## 2. How it was measured

- The new AI plays both sides of everything reported. The old AI was tested for screening and turned down for decks (section 10).
- **Random picker**: the sim's whole-run policy as it was. It takes the first card of every reward and draft, the first relic, buys the first thing it can afford and heals at every rest.
- **Ideal deck**: 18 spells and no starting-deck cards, at most two copies of a common or uncommon and one of a rare, with lands worked out as the game does. Played through whole acts taking no card and no relic (`sim --act`). The lists are data: `data/sim/ideal_decks.json`.
- **Lift**: single duels of random decks (the starting deck and 9 pool cards); a card's lift is the win rate of the decks that held it less that of the decks that didn't.
- Margins are 95%. 300 runs at 80 to 90% is +/- 3.5 to 4.7; 600 runs at 34% is +/- 3.8; a cell of a matchup table (100 duels) is +/- 6 to 10.
- Adding or removing any pool card changes what every seed offers, so two measurements of the same deck on "the same seeds" before and after a pool change differ by noise. The counters deck measured 80.7 and then 77.7 with nothing about it changed.

New sim modes (all in `CLAUDE.md`): `--lift`, `--ideal`, `--leave-out`, `--act`; `--runs` with `--no-cards`, `--no-relics`, `--with-relic`, `--with-card`; `--field`, `--data DIR` (data read at run time, no rebuild), `--threads N` (default three quarters of the cores: 12 here).

## 3. The criteria, one by one

### 1. Floor

`sim --runs --games 600 --decks warden`: **34.2%** of acts won (+/- 3.8). It reaches the boss in 82.8% of runs and beats it 41.2% of the time. It wins 73% of its ordinary duels and 19% of its elites.

Where it came from, in order of size: the boss's life (each 5 life costs the random deck about 4.5 points at the boss and a focused deck about 1), the relics (the four strong ones were worth 12 to 26 points in a single duel), the starting deck (5 points in single duels), and holding back the 22 generic cards.

### 2. Ceiling

`sim --act --games 300`:

| Ideal deck | Won the act | +/- | Reached the boss | Beat the boss | Other duels won | Elites won | Wounds taken |
|---|---|---|---|---|---|---|---|
| Go wide | 89.3 | 3.5 | 98.7 | 90.5 | 92.6 | 68.0 | 0.56 |
| Lifegain | 86.0 | 3.9 | 97.7 | 88.1 | 91.0 | 64.3 | 0.65 |
| Big creatures | 82.7 | 4.3 | 95.0 | 87.0 | 83.6 | 68.2 | 0.90 |
| Auras | 80.3 | 4.5 | 97.3 | 82.5 | 86.2 | 74.8 | 0.75 |
| +1/+1 counters | 77.7 | 4.7 | 93.3 | 83.2 | 86.4 | 63.8 | 0.83 |
| Random picker, with relics (600) | 34.2 | 3.8 | 82.8 | 41.2 | 73.1 | 19.4 | 1.67 |

On main's field these five won 56 to 81% of single duels and a random pile with a relic won 57%, so two of the "ideal" decks were worse than random. Now all five are 43 to 55 points above the floor.

Why three are short: the boss. Its emblem drains a life a turn, which a deck with lifegain shrugs off. Every list that reached the mid-80s did it by taking its package's bridge to lifegain (Impassioned Orator and Sworn Companions in wide; Battle Priest and Pridemate in counters; Honey Mammoth in big; Daybreak Coronet in auras). That is a real finding about the boss, and one of the questions in section 11.

The counters package is the weakest. A +1/+1 counter is a small effect, and its payoffs that should scale (Kalonian Hydra, High Sentinels, Hardened Scales, Abzan Falconer) are the kind the AI plays badly. Three rebuilt lists all landed at 71 to 78.

The lever that would close the gap, if you want it closed in the sim: boss life. At 40 life instead of 50 the ideal decks gain about 2 points each and the floor rises to about 42.

### 3. The whole act

| | Random picker | Ideal decks |
|---|---|---|
| Wounds taken per run | 1.67 | 0.56 to 0.90 |
| Dies before the boss (three wounds) | 17.2% | 1.3 to 6.7% |
| Arrives with no wound / one wound | 58.0% / 24.8% | 82 to 91% / 7 to 12% |
| Beats the boss, having arrived | 41.2% | 82.5 to 90.5% |

Nobody arrives with two wounds: the last row is a rest, and it heals one.

Of the gap between 34% and about 83%, the boss duel is much the larger part. Give the random picker the ideal decks' road to the boss and it would win about 40% of acts; give it their boss duel and it would win about 71%. The road counts for more than that sum shows, since the deck that meets the boss was built on it and a quarter of pickers arrive wounded, but the criterion as you wrote it is only partly met. Feats barely count yet: the picker's blind picks score 1.60 points a duel won (2.3 on main), because weaker decks meet fewer tiers, and the AI does not play for them. Feat and wound numbers were not changed.

If you want more of the gap to come before the boss, the lever is the wound itself (it deals 1 damage a turn; a harsher one, or a second kind, hurts the deck that loses more). I did not touch it.

### 4. No single relic, common or uncommon worth more than 5 points

**Relics**, act level: with relics 34.2%, without 27.0% (300 runs, +/- 5.0). The picker ends a run with 1.2 relics, so the average relic is worth about 6 points of act win rate. That is at the limit, not under it.

@@RELICLIFT@@

**Cards**, single duels:

`sim --lift --games 12000` on the final pool: the starting deck with 9 pool cards, each card as likely as the next, no relics, against the 14 enemies that are not easy. The random deck wins 53.5% of these duels. Each card is in about 1,000 decks, so each lift is +/- 3.2.

| Tier | Cards | Highest | Lowest | Every card at +3 or more |
|---|---|---|---|---|
| Common | 34 | Cartouche of Solidarity +4.4 | Rampant Growth -6.4 | Cartouche of Solidarity +4.4, Attended Knight +4.1, Sentinel Spider +3.5, Healer's Hawk +3.4, Honey Mammoth +3.2, Centaur Healer +3.1 |
| Uncommon | 41 | Impassioned Orator and Trostani's Summoner +3.8 | Angelic Accord -7.9 | Impassioned Orator +3.8, Trostani's Summoner +3.8, Angel of Renewal +3.7, Pacifism +3.5 |
| Rare | 29 | Archangel of Thune +7.6 | Hardened Scales -6.8 | Archangel of Thune +7.6, Resplendent Angel +5.6, Benalish Marshal +5.1, Kalonian Hydra +3.6, Setessan Champion +3.1 |

- **No common or uncommon measures above 5.** With a margin of 3.2 I can't rule out that the top two commons are at it.
- The one that was over is held back: Unflinching Courage, +10.2 (+/- 3.6) in an earlier run on this field.
- The two commons you named are inside the limit as measured (Honey Mammoth +3.2, Centaur Healer +3.1), so I left them. They are still the "verb on a good body" kind, and swapping them is cheap if you want the commons plainer.
- **Three rares are above 5** (section 6).

### 5. Flexible cards a slight edge, inflexible cards a big one in their deck

- The "all-stars" deck (`allstars` in the ideal decks file: Archangel of Thune, Kalonian Hydra, Thragtusk, Armada Wurm, Resplendent Angel, Steel Leaf Champion, two each of Celestial Unicorn, Ajani's Pridemate, Honey Mammoth, Pacifism and Leatherback Baloth, Glorious Anthem, Collective Blessing) wins 58.4% of single duels against the whole field and 52% against the boss. The package decks win 81 to 86% and 84 to 91%.
- The cards with the largest negative lift in a random deck are the build-arounds: Collective Unconscious, Hardened Scales, Sigil of the Empty Throne, Colossal Majesty, Gigantosaurus, Eidolon of Blossoms, Intangible Virtue, Garruk's Uprising.
- Three strong-anywhere rares were given a condition instead of being cut (section 6).

### 6. Tiers

Applied as the brief states them. Judgement calls, for you to overrule:

- Commons that do two verbs once each stay common: the Auras with a rider (Cartouche of Solidarity, Knightly Valor, Fists of Ironwood, Divine Favor), Sworn Companions and Honey Mammoth. Moving all the Aura bridges to uncommon would leave the package three commons.
- Unruly Mob stays common: it grows more than once, but only itself and slowly.
- Glorious Anthem is a rare with no condition. Benalish Marshal (the same effect on a body for {W}{W}{W}) was added as the conditional version and turned out the stronger of the two in a random deck. One of them should go.
- Armada Wurm, Steel Leaf Champion, Terra Stomper and Gigantosaurus are "conditional" only in colour: they ask for a green deck.

### 7. Relics

Section 8.

### 8 to 12. Fun

Not measured. What was done with each in mind:

- **8 (change direction; a wasted offer):** 22 cards are no longer offered (the starting deck's eight, eleven that belong to no package, three that are strong anywhere), so every offer but Pacifism and Smite the Monstrous is a package card. 28 of the 104 offered cards belong to two packages.
- **9 (a combination takes assembling):** Luminarch Aspirant now needs another counter source to start; Archangel of Thune needs 3 life in the turn; Thragtusk needs another big creature; Daybreak Coronet needs another Aura. The one to watch: Archangel's own lifelink is 3 life, so once it connects it feeds itself.
- **10 (nothing changes how a duel is played):** two things come close and are yours to judge: the boss at 50 life with a drain is a long race, and Fire Giants holds one Pyroclasm.
- **11 (plain commons):** 34 commons, none measured above +4.4 in a random deck.
- **12 (unique rares):** 12 rares added, each with a condition or a scale of its own (section 4). I looked more widely than I built: section 11 lists what I passed over and why.

## 4. The proposed pool, card by card

Rarity on Scryfall is per printing; ours is the tier. "Added" cards came through `tools/add-cards.mjs` and match Scryfall (`node tools/scryfall.mjs check`: 231 cards, 0 differences). Lifts quoted in the "Change" column are from the measurements you gave me or from this session's.

**Wide** (23 cards)

| Card | Tier | Package | Change |
|---|---|---|---|
| Attended Knight | common | wide |  |
| Call of the Conclave | common | wide |  |
| Captain's Call | common | wide |  |
| Inspired Charge | common | wide |  |
| Might of the Masses | common | wide |  |
| Raise the Alarm | common | wide |  |
| Sworn Companions | common | wide + lifegain |  |
| Angel of Renewal | uncommon | wide + lifegain | **Added.** A life for each creature as it enters. |
| Bestial Menace | uncommon | wide |  |
| Good-Fortune Unicorn | uncommon | wide + counters |  |
| Impassioned Orator | uncommon | wide + lifegain | **common to uncommon.** A repeatable enabler and a bridge: an uncommon by the tier rule. |
| Intangible Virtue | uncommon | wide |  |
| Jade Mage | uncommon | wide | **Added.** A Saproling for {2}{G}, as often as there is mana: the package had no repeatable token maker. |
| Overrun | uncommon | wide |  |
| Phantom General | uncommon | wide | **Added.** A second, smaller Intangible Virtue on a body (it was a spare of #59). |
| Trostani's Summoner | uncommon | wide + big |  |
| Woodland Champion | uncommon | wide + counters |  |
| Benalish Marshal | rare | wide | **Added.** An anthem that asks for a white deck ({W}{W}{W}). |
| Cathars' Crusade | rare | wide + counters |  |
| Champion of Lambholt | rare | wide + counters | **Added.** Grows with every creature that enters and makes the team hard to block: it scales by count and does nothing alone. |
| Collective Blessing | rare | wide |  |
| Collective Unconscious | rare | wide | **Added.** A card for each creature: nothing else in the pool refills a hand. |
| Glorious Anthem | rare | wide |  |

**Counters** (20 cards)

| Card | Tier | Package | Change |
|---|---|---|---|
| Crowned Ceratok | common | counters |  |
| Ironshell Beetle | common | counters |  |
| Lightwalker | common | counters |  |
| Pridemalkin | common | counters |  |
| Snakeskin Veil | common | counters |  |
| Unruly Mob | common | counters |  |
| Abzan Battle Priest | uncommon | counters + lifegain | **Added.** Lifelink for every creature with a counter: the bridge to lifegain. |
| Abzan Falconer | uncommon | counters | **Added.** Flying for every creature with a counter: the package had no way through a stalled board. |
| Armorcraft Judge | uncommon | counters |  |
| Basri's Solidarity | uncommon | counters |  |
| Conclave Mentor | uncommon | counters + lifegain |  |
| Inspiring Call | uncommon | counters | **Added** (a spare of #60). A card for each creature with a counter, and they are indestructible for the turn. |
| Invigorating Surge | uncommon | counters |  |
| Iridescent Hornbeetle | uncommon | counters + wide |  |
| Ridgescale Tusker | uncommon | counters + big |  |
| Avatar of the Resolute | rare | counters | **Added.** Enters with a counter for each other creature that has one. |
| Hardened Scales | rare | counters | **Added**, and a question for you: a second "that many plus one" (Conclave Mentor's rule, #61, on an enchantment). |
| High Sentinels of Arashin | rare | counters | **Added.** A flier that counts the other creatures with counters. |
| Kalonian Hydra | rare | counters |  |
| Luminarch Aspirant | rare | counters | **Text changed.** It was +4.6 in any deck. Now it only grows a creature that already has a counter, so it needs a second counter card to start. |

**Lifegain** (16 cards)

| Card | Tier | Package | Change |
|---|---|---|---|
| Angel of Mercy | common | lifegain |  |
| Centaur Healer | common | lifegain |  |
| Dawning Angel | common | lifegain |  |
| Healer's Hawk | common | lifegain |  |
| Revitalize | common | lifegain |  |
| Ajani's Pridemate | uncommon | lifegain + counters |  |
| Angelic Accord | uncommon | lifegain | **Added** (a spare of #61). An Angel in any turn with 4 life gained. |
| Celestial Unicorn | uncommon | lifegain | **common to uncommon.** A payoff that keeps growing is an uncommon by the tier rule (it is Ajani's Pridemate again). |
| Griffin Aerie | uncommon | lifegain |  |
| Nyx-Fleece Ram | uncommon | lifegain + auras |  |
| Soulmender | uncommon | lifegain | **common to uncommon.** A repeatable enabler is an uncommon by the tier rule. |
| Twinblade Paladin | uncommon | lifegain | **Text changed.** Decided before the session: it loses "whenever you gain life, put a +1/+1 counter" and keeps the double strike. |
| Angelic Chorus | rare | lifegain + wide |  |
| Archangel of Thune | rare | lifegain + counters | **Text changed.** It was +11.4 in any deck. Now its counters come at the end step of a turn with 3 life gained (the owner's idea). |
| Resplendent Angel | rare | lifegain |  |
| Speaker of the Heavens | rare | lifegain |  |

**Big** (23 cards)

| Card | Tier | Package | Change |
|---|---|---|---|
| Colossal Dreadmaw | common | big |  |
| Druid of the Cowl | common | big |  |
| Honey Mammoth | common | big + lifegain |  |
| Llanowar Elves | common | big |  |
| Rabid Bite | common | big |  |
| Rampant Growth | common | big |  |
| Rumbling Baloth | common | big |  |
| Sentinel Spider | common | big |  |
| Stampeding Rhino | common | big |  |
| Colossal Majesty | uncommon | big | **Added** (a spare of #62). A card every upkeep beside a creature of power 4. |
| Elemental Bond | uncommon | big | **Added** (a spare of #62). A card for each creature of power 3 or more. |
| Garruk's Uprising | uncommon | big |  |
| Leatherback Baloth | uncommon | big | **Added.** A 4/5 for {G}{G}{G}: only a green deck casts it on time. |
| Nessian Hornbeetle | uncommon | big + counters |  |
| Pelakka Wurm | uncommon | big + lifegain |  |
| Sight of the Scalelords | uncommon | big |  |
| Armada Wurm | rare | big |  |
| Gigantosaurus | rare | big |  |
| Overwhelming Stampede | rare | big + wide | **Added.** An Overrun as big as the biggest creature. |
| Spearbreaker Behemoth | rare | big | **Added.** Indestructible, and gives it to creatures of power 5 or more. |
| Steel Leaf Champion | rare | big |  |
| Terra Stomper | rare | big |  |
| Thragtusk | rare | big | **Text changed.** It was +3.9 in any deck. Now it gains 5 only beside another creature of power 4 or more (the owner's idea). |

**Auras** (20 cards)

| Card | Tier | Package | Change |
|---|---|---|---|
| Cartouche of Solidarity | common | auras + wide |  |
| Divine Favor | common | auras + lifegain |  |
| Fists of Ironwood | common | auras + wide |  |
| Gladecover Scout | common | auras |  |
| Knightly Valor | common | auras + wide |  |
| Setessan Training | common | auras |  |
| Ancestral Mask | uncommon | auras |  |
| Battle Mastery | uncommon | auras + big | **Added.** Double strike for a creature already made big. |
| Ethereal Armor | uncommon | auras |  |
| Oakheart Dryads | uncommon | auras | **common to uncommon.** It pumps for every enchantment that enters: a repeatable payoff, so an uncommon. |
| Rancor | uncommon | auras |  |
| Satyr Enchanter | uncommon | auras |  |
| Yavimaya Enchantress | uncommon | auras | **Added** (a spare of #63). Grows with every enchantment. |
| Celestial Ancient | rare | auras + counters | **Added.** A counter on every creature for each enchantment cast. |
| Daybreak Coronet | rare | auras + lifegain | **Added.** +3/+3, first strike, vigilance and lifelink, but only for a creature that already has an Aura. |
| Eidolon of Blossoms | rare | auras | **Added** (a spare of #63). A card for each enchantment. |
| Kor Spiritdancer | rare | auras | **Added**, with one word changed ("draw a card" for "you may draw a card"). +2/+2 for each Aura on it, and a card for each Aura cast. |
| Mantle of the Wolf | rare | auras |  |
| Setessan Champion | rare | auras + counters |  |
| Sigil of the Empty Throne | rare | auras |  |

**For any deck** (2 cards)

| Card | Tier | Package | Change |
|---|---|---|---|
| Smite the Monstrous | common | any deck |  |
| Pacifism | uncommon | any deck | **common to uncommon.** The pool's one cheap answer to anything (+3.8 in any deck): rarer, not gone. |

**Held back: never offered** (22 cards; enemies and the starting deck still use them)

| Card | Tier | Why |
|---|---|---|
| Aerial Responder | uncommon | Strong in any deck (+5.4, the top of the list). Candidate for an event. |
| Ambush Viper | common | Belongs to no package. |
| Angelic Wall | common | Belongs to no package. |
| Assault Griffin | common | Belongs to no package. |
| Briarpack Alpha | uncommon | Belongs to no package. |
| Centaur Courser | common | A starting-deck card: not offered again. |
| Elvish Visionary | common | A starting-deck card: not offered again. |
| Fencing Ace | uncommon | Belongs to no package. |
| Giant Growth | common | A starting-deck card: not offered again. |
| Giant Spider | common | A starting-deck card: not offered again. |
| Glory Seeker | common | A starting-deck card: not offered again. |
| Grizzly Bears | common | A starting-deck card: not offered again. |
| Inspiring Overseer | common | A flier, a life and a card at common (+4.0). Candidate for an event. |
| Plummet | common | Belongs to no package, and dead against most enemies. |
| Savannah Lions | uncommon | Belongs to no package. |
| Serra Angel | uncommon | A plain strong body that fits every deck and belongs to none. |
| Sheltering Light | uncommon | Belongs to no package. |
| Siege Mastodon | common | A starting-deck card: not offered again. |
| Thornweald Archer | common | Belongs to no package. |
| Titanic Growth | common | Belongs to no package. |
| Unflinching Courage | uncommon | Strong in any deck (+10.2 +/- 3.6 on the new field): trample and lifelink on anything. Candidate for an event. |
| Youthful Knight | common | A starting-deck card: not offered again. |

Offered: 104 cards (34 common, 41 uncommon, 29 rare).

## 5. Each package's ideal deck

Eighteen spells; the game adds 12 lands.

- **Go wide** (`wide`): 2 Raise the Alarm, 2 Attended Knight, 2 Captain's Call, 2 Sworn Companions, Call of the Conclave, Trostani's Summoner, 2 Impassioned Orator, 2 Intangible Virtue, Champion of Lambholt, Glorious Anthem, Benalish Marshal, Collective Blessing.
- **+1/+1 counters** (`counters`): 2 Conclave Mentor, 2 Abzan Battle Priest, 2 Ajani's Pridemate, 2 Ironshell Beetle, 2 Good-Fortune Unicorn, Abzan Falconer, Lightwalker, Pridemalkin, Snakeskin Veil, Basri's Solidarity, Luminarch Aspirant, Kalonian Hydra, Archangel of Thune.
- **Lifegain** (`lifegain`): 2 Healer's Hawk, 2 Ajani's Pridemate, 2 Celestial Unicorn, Impassioned Orator, Centaur Healer, Nyx-Fleece Ram, Angel of Mercy, Dawning Angel, Angelic Chorus, Angelic Accord, Griffin Aerie, Twinblade Paladin, Speaker of the Heavens, Resplendent Angel, Archangel of Thune.
- **Big creatures** (`big`): 2 Llanowar Elves, 2 Druid of the Cowl, 2 Leatherback Baloth, 2 Rabid Bite, Garruk's Uprising, Colossal Majesty, Ridgescale Tusker, 2 Honey Mammoth, Thragtusk, Steel Leaf Champion, Armada Wurm, Terra Stomper, Pelakka Wurm.
- **Auras and enchantments** (`auras`): 2 Gladecover Scout, 2 Cartouche of Solidarity, 2 Divine Favor, Nyx-Fleece Ram, 2 Ethereal Armor, Ancestral Mask, 2 Satyr Enchanter, Rancor, Daybreak Coronet, Kor Spiritdancer, Setessan Champion, Sigil of the Empty Throne, Mantle of the Wolf.

Leaving cards out showed which carry a deck (40 duels a cell, 8 enemies, +/- 5 a row). In the counters deck only Conclave Mentor (-8 to -10 for the pair), Abzan Battle Priest (-5 to -6.5) and Luminarch Aspirant (-4 to -8.5) were worth more than the Grizzly Bears put in their place. In the wide deck it was the token makers and Intangible Virtue (-7 to -9 each pair).

## 6. Strong and versatile

**Held back (`"reserved": true`): candidates to lock behind an event.**

| Card | Why |
|---|---|
| Aerial Responder | +5.4 in any deck, the top of the list on main's field |
| Inspiring Overseer | +4.0: a flier, a life and a card at common |
| Unflinching Courage | +10.2 (+/- 3.6) on the new field: the one uncommon clearly over the limit |
| Serra Angel | A strong plain body that belongs to no package |
| War Banner (relic) | +22 to +26 in a single duel |
| Surveyor's Map (relic) | +25 in a single duel |

**Given a condition instead of cut** (each file is marked `altered`, so the Scryfall check skips its text and nothing else):

| Card | Was | Now |
|---|---|---|
| Archangel of Thune | Whenever you gain life, a counter on each creature you control (+11.4) | At the beginning of your end step, if you gained 3 or more life this turn, the same |
| Thragtusk | Gains 5 as it enters (+3.9) | Gains 5 only if you control another creature with power 4 or greater |
| Luminarch Aspirant | A counter on any creature of yours each combat (+4.6) | Only on a creature that already has a +1/+1 counter |
| Twinblade Paladin | Counter whenever you gain life, and double strike | Double strike only (decided before the session) |
| Kor Spiritdancer (new) | "you may draw a card" | "draw a card": the engine can't decline an ability that has no target |

**Still in the pool and worth watching** (rares; the lift of a rare is measured on few decks, so read the margins):

| Card | Lift in a random deck (+/- 3.2) | Note |
|---|---|---|
| Archangel of Thune | +7.6 | Still the strongest card for any deck, with the condition on. Its own lifelink meets it. Lock it behind an event, or ask for 4 or 5 life |
| Resplendent Angel | +5.6 | The 3/3 flier for three is the value, not the Angels it rarely makes |
| Benalish Marshal | +5.1 | I added it as the conditional anthem. It measures as more versatile than Glorious Anthem (+1.9): of the two, this is the one to cut |
| Kalonian Hydra | +3.6 | +5.2 on main's field by your measure |

## 7. The enemy field

Five tiers. `hard` is new: the duels of rows 9 and 10 (`balance.json`). Elites and the boss have commanders. All decks are 25 cards with 10 lands. Tiers follow what the sim measured (#42).

| Enemy | Tier | Colours | Identity |
|---|---|---|---|
| Forest Pack | easy | G | Small green creatures and one trick |
| Sky Patrol | easy | WU | Cheap fliers |
| Grave Robbers | easy | B | Small creatures that trade: deathtouch, discard |
| Goblin Raid | easy (was normal) | R | A rush of one- and two-drops |
| Storm Scholars | easy | UR | Spells and prowess |
| Border Legion | normal | RW | First strike, haste, burn |
| Blood Cult | normal | BR | Sacrifice around Mayhem Devil |
| Tide Weavers | normal | UB | Bounce, counterspells, fliers |
| Dragon Brood | normal (new) | R | Goblins, then firebreathing Whelps |
| Bog Stalkers | normal (new, for Rot Farmers) | BG | Deathtouch and cards back from the graveyard |
| Primal Stampede | hard (was normal) | GU | Ramp into big creatures |
| Grove Wardens | hard (was elite) | GW | Tokens under Glorious Anthem |
| Ashen Coven | hard (new) | B | Drain: every creature takes life as it enters |
| Fire Giants | hard (was normal) | R | Four- and five-power bodies, Shivan Dragon, one Pyroclasm |
| Town Militia | elite | W | Kongming: an army of small creatures, all +1/+1 |
| Plague Court | elite | WB | Elas il-Kor: life for every creature in, life lost for every one that dies |
| Angelic Host | elite | WU | Radiant, Archangel (new): grows with every flier on the battlefield |
| Wild Hunt | elite (was normal) | RG | The Lady of the Mountain: big creatures and Rabid Bite |
| The Pyre Tyrant | boss | RG | Borborygmos (new): a weak early deck, 50 life, and the Pyre (you lose 1 life each of its upkeeps) |

Rot Farmers is gone: it milled itself with no payoff. Sir Shandlar and Tobias Andrion lead nobody now; their card files are kept.

**Why the late decks look like this.** On the first rebuilt field I compared a random deck with the package decks enemy by enemy. Against aggro, burn and removal decks the random pile did as well as a package deck. The gap was 19 to 28 points against the decks that keep growing: commander-led elites and big-creature decks. So aggro and tempo are the normal tier, and the hard tier, the elites and the boss are decks with inevitability.

**Matchups.** Win rate of each deck in single duels, no relics, 100 duels a cell (a cell is +/- 6 to 10; the last row +/- 1.6 to 2.2). "start" is the bare starting deck.

| Enemy | Tier | start | wide | counters | lifegain | big | auras | all-stars |
|---|---|---|---|---|---|---|---|---|
| Forest Pack | easy | 81 | 96 | 92 | 99 | 91 | 90 | 74 |
| Sky Patrol | easy | 83 | 90 | 85 | 94 | 91 | 88 | 61 |
| Grave Robbers | easy | 68 | 99 | 92 | 98 | 92 | 95 | 89 |
| Goblin Raid | easy | 93 | 96 | 88 | 93 | 78 | 84 | 58 |
| Storm Scholars | easy | 85 | 91 | 80 | 92 | 81 | 82 | 65 |
| Border Legion | normal | 84 | 92 | 81 | 91 | 82 | 85 | 61 |
| Blood Cult | normal | 64 | 92 | 90 | 92 | 89 | 86 | 70 |
| Tide Weavers | normal | 60 | 91 | 82 | 97 | 83 | 88 | 60 |
| Dragon Brood | normal | 74 | 83 | 85 | 89 | 75 | 78 | 55 |
| Bog Stalkers | normal | 6 | 99 | 85 | 93 | 88 | 88 | 68 |
| Primal Stampede | hard | 8 | 88 | 81 | 78 | 94 | 89 | 61 |
| Grove Wardens | hard | 11 | 87 | 76 | 82 | 88 | 88 | 57 |
| Ashen Coven | hard | 11 | 83 | 85 | 91 | 88 | 88 | 54 |
| Fire Giants | hard | 27 | 86 | 87 | 88 | 90 | 91 | 76 |
| Town Militia | elite | 0 | 60 | 59 | 63 | 70 | 74 | 28 |
| Plague Court | elite | 1 | 77 | 85 | 85 | 78 | 85 | 43 |
| Angelic Host | elite | 14 | 52 | 62 | 42 | 62 | 73 | 25 |
| Wild Hunt | elite | 1 | 76 | 63 | 73 | 83 | 79 | 52 |
| The Pyre Tyrant | boss | 1 | 91 | 85 | 87 | 84 | 88 | 52 |
| **All 19** | | **40.6** | **85.7** | **81.2** | **85.6** | **83.5** | **85.2** | **58.4** |

- Every package has enemies it beats nine times in ten and enemies it struggles with. The worst cell of a package deck is 42 (lifegain against Angelic Host). Nobody is shut out.
- Angelic Host is every package's worst or near-worst matchup except auras'. It is the weakest I could make it while Radiant leads it; say so if an elite at 42 to 73 is too hard.
- The bare starting deck loses to everything from Bog Stalkers on (0 to 27%). It is not meant to meet them bare: the first of them comes after two rewards and a draft.
- Bog Stalkers is in the normal tier on the strength of what drafted decks do against it (85 to 99). Against the bare starting deck it is the hardest "normal" enemy by far (6%).

## 8. Relics

Thirteen are offered; two are held back. Tiers are my suggestion and exist nowhere in the code.

| Relic | Text | Glue | Suggested tier | Change |
|---|---|---|---|---|
| Traveller's Rations | Start each duel with 3 more life | any | common | |
| Rally Horn | The first creature token to enter each turn brings a 1/1 Soldier with it | wide | common | **Was** a 1/1 Soldier at the start of every duel (+10, +/- 4, in a single duel: the strongest of the small relics) |
| Pilgrim's Flask | First life gained each turn: scry 1 | lifegain | common | |
| Soothing Spring | The first time you gain life each turn, gain 1 more | lifegain | common | **Was** 1 life every upkeep (+16 points by your measure, +23 in a single duel) |
| Beast Tamer's Whistle | A creature of power 4 or more enters: gain 1 | big + lifegain | common | **Was** gain 2. Held from the first duel it took the picker from 27.7% to 41.3% of acts: the commander is a creature of power 4, so it is always on |
| Votive Candle | First enchantment spell each turn: a 1/1 Soldier | auras + wide | common | |
| Weaver's Thread | First Aura spell each turn: gain 2 | auras + lifegain | common | **New** |
| Sentinel's Horn | Your creatures with a +1/+1 counter have vigilance | counters | common | **Was** vigilance for every creature |
| Whetstone | The first creature token to enter each turn gets a +1/+1 counter | wide + counters | rare | **Was** the first creature of any kind (+26 in a single duel) |
| Drover's Bell | The first creature of power 4 or more to enter each turn: a 1/1 Soldier | big + wide | rare | **New** |
| Anointing Oil | First life gained each turn: a counter on each creature of yours that has one | lifegain + counters | rare | **New** |
| Lucky Coin | 10 more gold a duel won | the run | common | |
| Merchant's Ledger | Shop prices 20% lower | the run | common | |
| War Banner | Creatures you control get +1/+0 | any | boss, or an event | **Held back** |
| Surveyor's Map | Start each duel with a basic land | any | boss, or an event | **Held back** |

A relic can now be `"reserved": true` like a card; no treasure, shop, elite or event offers it.

## 9. What was added and changed

### Rules added to the engine (each with tests)

1. **Damage to each creature** (`deal_damage_to_each_creature`): Pyroclasm.
2. **Count: other creatures with flying, either side** (`other_creature_with_flying`): Radiant, Archangel.
3. **Count: your other creatures with a +1/+1 counter** (`other_creature_you_control_with_counter`): High Sentinels of Arashin, Avatar of the Resolute.
4. **Count: Auras attached to this permanent** (`aura_attached_to_this`): Kor Spiritdancer.
5. **"Creatures with power less than this creature's power can't block creatures you control"** (`weaker_creatures_cant_block`), checked where a block is declared: Champion of Lambholt.
6. **Target filter "with a +1/+1 counter on it"** (`with_counter` on a creature filter): the altered Luminarch Aspirant.
7. **Enchant "creature with another Aura attached to it"** (`enchanted` on a creature filter): Daybreak Coronet. It can only be cast on a creature that has an Aura, and falls off when it is the only one left.
8. **"Enters with a +1/+1 counter for each..."** (`enters_with_counters_for_each`): Avatar of the Resolute.
9. **"+X/+X, where X is the greatest power among creatures you control"** (`greatest_power` on the effect for all your creatures): Overwhelming Stampede.
10. **"Those creatures": only your creatures with a counter** (`with_counter` on the same effect): Inspiring Call.
11. **Life for each thing counted** (`for_each` on `gain_life`): Angel of Renewal.
12. **Trigger on casting an Aura spell** (`cast_spell` kind `aura`): Kor Spiritdancer, Weaver's Thread.

No rule marked Out or Later in `RULES_SCOPE.md` was built. Hardened Scales reuses Conclave Mentor's "that many plus one" (#61 said each further such card is asked about on its own: question 4).

### Not rules, but new in the engine

- A card or a relic may be `"reserved": true`: never offered.
- An enemy tier `hard`; with no hard enemy in the data a hard row meets a normal one.
- A character need not have a starting emblem, and `emblems.json` is empty. The menu hides the choice when there is none.
- `data::replace_embedded`, so that the sim can load data from a folder. The engine still does no file IO.

### Numbers

- `balance.json`: rows 9 and 10 of the map meet `hard` enemies (`duel_tier`). One number is new and is the sim's own: `sim.board_cap` (90), which stops a single-duel game with more than 90 permanents on the battlefield and counts it like one past the turn cap. One game in a lift run stalled on such a board for over 20 minutes of AI thinking. Nothing else in the file changed: wounds, feats, gold, rewards, weights, prices and the map's shape are as they were.
- `enemies.json`: the boss's Tyrant's Vigour is **30** more life (was 5), so it starts at 50.
- `planeswalkers.json`: the Warden's starting deck is 2 Glory Seeker, 2 Grizzly Bears, Youthful Knight, Elvish Visionary, Centaur Courser, Giant Spider, Siege Mastodon, Giant Growth. **Out:** Savannah Lions, Pacifism, Rumbling Baloth. Still 10 spells and 7 lands.
- `emblems.json`: empty (Warm Welcome and Battle Fury removed, as decided).

### Cards

38 card files added, 31 changed. Of the 38: 24 for the pool (section 4) and 14 for enemies. The 11 black and red enemy cards are `reserved` so that the Firebrand's rewards are what they were.

Six of the enemy cards ended up in no deck when the field was weakened: Flametongue Kavu, Lightning Bolt, Volcanic Dragon, Gifted Aetherborn, Liliana's Specter, Eviscerate. They are tested and held back, ready for an act-two enemy. Delete them if you would rather not carry them.

## 10. What was tried and didn't work

- **The old AI for decks.** It plays the plain starting deck 15 points better than the new AI's opponent allows and the synergy decks 5 to 13 points worse, so it ranks the starting deck above the counters deck. For single cards in random decks it agrees with the new AI as well as two noisy measurements can (correlation 0.67 over 102 cards), but I ended up not needing it.
- **Ideal decks made only of their package's cards.** The first counters and big lists (53 and 54% in single duels) lost to a random deck with a relic (57%).
- **A weak boss.** Taking the ramp and the big creatures out of the boss's deck raised the ideal decks by 9 points and the random picker by 25 (to 57.7% of acts).
- **A boss that grows.** "A +1/+1 counter on each of its creatures every upkeep": ideal decks 36%, random 4.5%. It separates, and it is no fun.
- **More boss life without the drain.** At 40, 50 and 60 life and no drain the random deck still wins 64 to 72% against it. The drain is what makes the life matter.
- **A removal deck in the hard tier.** The first Ashen Coven (three Murders, Doom Blade, Eviscerate, deathtouch) beat the counters deck 94% of the time: a shut-out. It is a drain deck now.
- **Weakening the normal and hard decks to raise the ceiling.** It raises the random deck as much as the ideal ones.
- **Engines in AI hands.** Cathars' Crusade costs the wide and counters decks 4 to 5 points against a Grizzly Bears in its place; Hardened Scales, High Sentinels, Ridgescale Tusker and Abzan Falconer are worth no more than the Bears. I left them in the pool, because this is the AI's limit and not the cards'.
- **Yavimaya Enchantress and Battle Mastery in the auras deck**: 6 points worse than the hexproof Gladecover Scouts they replaced.
- **New rares in the lists.** Avatar of the Resolute and Inspiring Call (counters), Overwhelming Stampede (big), Jade Mage and Angel of Renewal (wide) changed nothing measurable (150 runs, +/- 7). Daybreak Coronet and Kor Spiritdancer in place of Unflinching Courage did help auras (80 to 85 on 150 runs; 80.3 on 300).
- **A relic without "once a turn".** Drover's Bell first read "whenever a creature with power 4 or greater enters". Under Collective Blessing its own Soldier is a 4/4, so it rang itself for ever; one sim game sat on that for 20 minutes. It is once a turn now, with a test, and the sim stops any single duel with more than 90 permanents on the battlefield.
- **Three counters lists** (the current one; a hybrid with Celestial Unicorn and both Falconers; one with both Tuskers): 77.7, 71.0 and 77.0 on 300 runs each.

## 11. Questions that are yours

1. **The boss at 50 life.** You said to use real decks, not extra life. I kept to that for 18 enemies and broke it for the boss, where life turned out to be the one lever that hurts a random deck four times as much as a focused one. Is a 50-life boss with a 1-a-turn drain the fight you want? The table: 20 life, ideal 88.8 / random 68.3; 30, 87.0 / 55.7; 35, 86.6 / 46.8; 40, 85.4 / 42.5; 45, 84.2 / 37.3 (single duels against it). The floor at 34% needs about 50.
2. **The Pyre makes lifegain the answer.** Every package's best list leans on its lifegain bridge. Do you want a boss that asks one question of every deck, or should the drain go and something else separate (and then what)?
3. **The starting deck.** Lions, Pacifism and the Baloth are out. The bare deck wins 68 to 93% against the easy decks, so the first two duels are no longer free. Is that the "weak start" you meant?
4. **Hardened Scales.** A second "that many plus one". It needs your word under #61. With two Mentors it is three extra counters a time.
5. **The three altered rares** (Archangel, Thragtusk, Aspirant). Archangel and Thragtusk are your wordings. The Aspirant's is mine.
6. **22 cards held back.** The starting deck's eight cards and eleven package-less ones are never offered. Is "starter cards are not rewards" right for this game?
7. **Event candidates**: Aerial Responder, Inspiring Overseer, Unflinching Courage, Serra Angel, War Banner, Surveyor's Map.
8. **Glorious Anthem or Benalish Marshal.** Same effect. I added the Marshal as the conditional one ({W}{W}{W}), and it measured as the stronger in a random deck (+5.1 against +1.9). Keep one.
9. **Commanders.** Radiant, Archangel leads Angelic Host; Borborygmos leads the boss; The Lady of the Mountain moved to Wild Hunt. Sir Shandlar and Tobias Andrion are out of work.
10. **Relic tiers.** A suggestion in section 8. Building them means deciding where each tier comes from (treasure, elite, shop, event).
11. **Feats.** Untouched, and the picker's blind score fell from 2.3 to 1.60 a duel because its deck is weaker. Tempered and Adorned are met in 6 to 7% of wins. They want a pass once you have played this.
12. **The Firebrand.** 2.7%. Do you want it kept playable while the Warden is balanced (an easier field for it, or its emblem back), or parked?
13. **Rares I looked at and did not add**, for want of a rule or of time: Beastmaster Ascension (a threshold of 7 attacks, then +5/+5: needs a counter on an enchantment); Enduring Scalelord (grows whenever another creature gets counters: needs a trigger on counters being put); Crusader of Odric and Geist-Honored Monk (power equal to your creature count: the Scryfall tool refuses a `*` power); Serra Ascendant (a 6/6 flier at 30 life: a second Speaker of the Heavens); Craterhoof Behemoth (8 mana, never cast in a 30-card deck); Elite Scaleguard (needs bolster); Citadel Siege and Verdurous Gearhulk (modes, dividing counters). Say which you want.

## 12. State of the branch

- `balance-lab`, small commits, nothing pushed, nothing on `main`.
- `.\tools\check-all.ps1`: @@CHECK@@
- `docs/ARCHITECTURE.md` and `CLAUDE.md` describe the new data fields, the tier and the sim modes, because the code on this branch needs them. `DECISIONS.md`, `ROADMAP.md`, `RULES_SCOPE.md` and `GAME_DESIGN.md` are untouched.
- Not done: relic tiers in the engine; any tuning of feats or wounds; a second look at the easy tier; the Firebrand.
- Not verified by anything but the sim and the tests: I did not play a duel. The end-to-end tests click through two whole acts on the new data and pass.

## Appendix: how the field got here

Single duels unless it says acts. All with the new AI.

| Step | What changed | Result |
|---|---|---|
| Main, emblems removed | | start 38, wide 70, counters 61, lifegain 81, big 56, auras 76 (12 non-easy enemies) |
| Field v1 | 5 easy, 5 normal, 4 hard, 4 elite, boss; 14 enemy cards | Ashen Coven and Bog Stalkers far too strong |
| Field v2 | Those two and three elites weakened | start 27, wide 69, counters 53, lifegain 78, big 54, auras 75; **random with a relic 57** |
| Relics | | Against the two that do nothing in a duel (45%): Whetstone 71.5, Surveyor's Map 69.7, Soothing Spring 68.7, War Banner 67.7; the rest 49 to 54 |
| Starting deck | Lions, Pacifism, Baloth out | random without a relic 36.3 to 31.0 |
| Field v3 | Aggro and tempo to normal; the hard tier from decks that scale; generic cards held back; relics tamed | packages 65 to 80, all-stars 52, random with a relic 44 |
| v4, acts | Boss: ramp, fat, Borborygmos, 20 life | wide 62, counters 54, lifegain 82, big 77, auras 73; picker **23.0** |
| v5, acts | Weaker easy decks and boss | 77, 59, 79, 73, 71; picker **37.0** |
| v6, acts | Boss without ramp or fat; weaker normal and hard decks | 79, 69, 89, 84, 81; picker **57.7** |
| v7, acts | Boss at 40 life; weaker elites | 79, 63, 90, 76, 76; picker **41.7** |
| Acts | Boss at 45; counters deck takes its lifegain bridge | 75, 81, 90, 74, 79; picker **37.7** |
| Acts | Wide and big take theirs | wide 90, big 84 |
| Final | Boss at 50; Unflinching Courage held back; Coronet and Spiritdancer in auras | **89.3, 77.7, 86.0, 82.7, 80.3; picker 34.2** |
