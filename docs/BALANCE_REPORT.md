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
