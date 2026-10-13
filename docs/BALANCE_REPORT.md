# Balance report: the Warden's first act

A proposal from one overnight session on the branch `balance-lab`. Nothing here is settled: `DECISIONS.md` and `ROADMAP.md` are untouched.

**Status: work in progress. This file is rewritten at the end; until then the working log at the bottom is the record.**

## Working log

### Tools (done)

- `sim --lift`, `--ideal`, `--leave-out`, `--act`, and `--runs` with `--no-cards`, `--no-relics`, `--with-relic`, `--with-card`; `--data DIR` loads data at run time; `--threads N` (default three quarters of the cores: 12 here).
- Ideal decks are data: `data/sim/ideal_decks.json`.
- Engine: `data::replace_embedded`, a card flag `reserved` (never offered), `run::offered_spells`, an enemy tier `hard`, the starting emblem optional.

### Old AI against new AI (the pool and field of main, emblems removed)

Ideal decks in single duels against the 12 non-easy enemies, 100 duels a cell (each "all" figure +/- 2.8):

| Deck | new AI | old AI |
|---|---|---|
| start | 38.3 | 53.1 |
| wide | 70.2 | 56.9 |
| counters | 60.6 | 47.9 |
| lifegain | 81.4 | 76.2 |
| big | 56.2 | 56.7 |
| auras | 76.3 | 67.8 |

The old AI plays the plain starting deck 15 points better than the new AI's opponent lets it and the synergy decks 5 to 13 points worse, so it puts the starting deck above the counters deck. **It is not used for decks or acts.**

Card lift (starting deck + 9 random pool cards, same field): old AI 24,000 duels (+/- 2.2 a card), new AI 16,000 duels (+/- 2.7 a card). The two lists correlate at 0.67, about what two noisy measurements of the same ranking give; the same cards lead both (Aerial Responder, Inspiring Overseer, Archangel of Thune, Pacifism). **The old AI is used to screen single cards only**, and anything that matters is measured again with the new AI.

New-AI lift on main's field, the ends of the list: Aerial Responder +5.4, Archangel of Thune +4.2, Angel of Mercy +4.1, Inspiring Overseer +4.0, Pacifism +3.8, Unflinching Courage +3.7, Thragtusk +3.7 ... Gigantosaurus -4.9, Rampant Growth -5.1, Cathars' Crusade -6.1, Intangible Virtue -6.2.

Speed: the new AI plays about 16 lift duels a second on 12 threads (16,000 in 17 minutes); the old AI about 250.

### Field v1 and v2, and what they showed (01:00 to 01:40)

- Field v1 (5 easy, 5 normal, 4 hard, 4 elite, boss) had two decks far too strong (Ashen Coven: ideal decks won 6 to 48%; Bog Stalkers beat the starting deck 93%). v2 weakened them and three elites.
- On v2, single duels, new AI, 100 a cell (+/- 2.3 on "all"): start 27, wide 69, counters 53, lifegain 78, big 54, auras 75.
- **A random deck with one relic beats two of the five "ideal" decks.** Starting deck + 9 cards picked by reward weights + 1 random relic: 56.9% (+/- 1.5) against the 14 non-easy decks of v2. Without a relic, on the 8-deck probe field: 36.3% (+/- 2.4), where the ideal decks win 55 to 71%.
- Relics in single duels (each in about 350 of 4200 decks, +/- 5): against the two that do nothing in a duel (45%), Whetstone 71.5, Surveyor's Map 69.7, Soothing Spring 68.7, War Banner 67.7; the Horns, the Candle and the Whistle about 54; Rations 52; Flask 49.
- An "all-stars" deck (the best 18 cards of every package) wins 69% (+/- 4) on the probe field: no 18-card deck gets near 90% against this field in AI hands, so the field has to come down for the ceiling, and the floor has to come from a weaker start, tamer relics and wounds.
- Where random and ideal differ most (v2): against the commander-led elites and the big-creature decks (Primal Stampede +19, Plague Court +28, Angelic Host +24, Wild Hunt +28). Against aggro, burn and removal decks a random pile does as well as a package deck. So the late field (v3) is built from decks that scale, and the aggro and tempo decks are the normal tier.
- Leave-one-out, counters deck (40 a cell, +/- 5.4 a row): only Luminarch Aspirant (-8.5), Conclave Mentor x2 (-8) and Abzan Battle Priest (-5) carry it; Hardened Scales, the Unicorn, Solidarity, the Tusker and High Sentinels are worth no more than Grizzly Bears; Cathars' Crusade is worth less (+5 without it). Wide deck: the token makers and Intangible Virtue carry it; Crusade again costs it 4.
- A weaker starting deck (Lions, Pacifism and the Baloth out; a second Seeker and Bears and a Siege Mastodon in): the random deck falls from 36.3 to 31.0 on the probe field.

### Field v3 to v7 (01:40 to 03:40)

- v3 moved the aggro and tempo decks to the normal tier and built the hard tier from decks that scale. With the generic strong cards held back, the weaker starting deck and the tamed relics: ideal decks 65 to 80% in single duels, the "all-stars" pile 52%, the random deck with a relic 44%.
- Whole acts, 150 runs a deck (+/- 7), 300 for the random picker (+/- 5.5):

| Step | wide | counters | lifegain | big | auras | random picker |
|---|---|---|---|---|---|---|
| v4 (boss: ramp, fat, Borborygmos, no extra life) | 62 | 54 | 82 | 77 | 73 | 23.0 |
| v5 (weaker easy decks and boss; generic cards reserved) | 77 | 59 | 79 | 73 | 71 | 37.0 |
| v6 (boss without ramp or fat; weaker normal and hard decks) | 79 | 69 | 89 | 84 | 81 | 57.7 |

- v6 showed the wrong lever: a weak boss let the random picker beat it 69.5% of the time. Boss variants in single duels (ideal decks 100 duels each, the random deck with a relic 600):

| Boss | ideal decks, mean of five | random deck |
|---|---|---|
| v6 deck, 20 life | 88.8 | 68.3 |
| v6 deck, 30 life | 87.0 | 55.7 |
| v6 deck, 35 life | 86.6 | 46.8 |
| v6 deck, 40 life | 85.4 | 42.5 |
| v6 deck, 45 life | 84.2 | 37.3 |
| ramp deck, 20 life | 80.2 | 50.3 |
| v6 deck, 20 life, a +1/+1 counter on its creatures every upkeep | 35.8 | 4.5 |

- So v7's boss is the weak early deck led by Borborygmos, the Pyre's drain, and 40 life: more life costs a focused deck about a point per 5 life and the random deck about 6.
