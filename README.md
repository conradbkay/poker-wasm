# power-range-rs

This is a rust port of the algorithm in <https://github.com/conradbkay/poker-utils> for calculating the equity of a Hold'em range vs a second range

## Benchmarks

The benchmark randomizes the board and both ranges for each trial

![](assets/lines.svg)

## Usage

```bash
npm install poker-wasm
```

### NLHE Equity Calculation

Calculate equity for a Hold'em range vs another range:

```ts
import { EquityCalculator, HoldemRange } from "poker-wasm"
import fs from "fs"

// download from here: https://github.com/chenosaurus/poker-evaluator/blob/master/data/HandRanks.dat
const handRanksData = fs.readFileSync("./HandRanks.dat")
const calculator = new EquityCalculator(handRanksData)

// Create ranges
const heroRange = new HoldemRange()
const vsRange = new HoldemRange()

// Add hands to ranges (cards are 0-51)
// For example, AA = [51, 50], KK = [47, 46], etc.
heroRange.set_hand(new Uint8Array([51, 50]), 1.0) // AA with 100% weight
heroRange.set_hand(new Uint8Array([47, 46]), 1.0) // KK with 100% weight

vsRange.set_hand(new Uint8Array([43, 42]), 1.0) // QQ
vsRange.set_hand(new Uint8Array([39, 38]), 1.0) // JJ

// Set ranges once (avoids repeated memory transfers)
calculator.setHeroRange(heroRange)
calculator.setVsRange(vsRange)

// Calculate equity on a flop
const board = new Uint8Array([0, 12, 28])
const results = calculator.equity_vs_range(board)

// Results contain equity for each hand in hero's range
results.forEach((result) => {
  const { combo, equity } = result
  console.log(`Hand: [${combo}]`)
  console.log(`  Win: ${equity.win.toFixed(3)}`)
  console.log(`  Tie: ${equity.tie.toFixed(3)}`)
  console.log(`  Lose: ${equity.lose.toFixed(3)}`)
})
```

### Omaha (PLO) Range vs Range Equity

Calculate PLO equity for a hero range vs a villain range. The board may be 3, 4,
or 5 cards; incomplete boards are enumerated. On a flop, passing `maxRunouts`
samples that many turn/river runouts (Monte Carlo) instead of enumerating all of
them. Works for PLO4, PLO5, and PLO6 (the range's hand size sets the variant).

```ts
import { EquityCalculator, OmahaRange } from "poker-wasm"
import fs from "fs"

// Load the hand evaluation data file
const handRanksData = fs.readFileSync("./HandRanks.dat")
const calculator = new EquityCalculator(handRanksData)

// Hero and villain ranges (4-card hands here). Set once to avoid repeat transfers.
const heroRange = new OmahaRange(4)
heroRange.addHand(new Uint8Array([35, 34, 31, 30]), 1.0) // TT99

const vsRange = new OmahaRange(4)
vsRange.addHand(new Uint8Array([51, 50, 47, 46]), 1.0) // AAKK double suited
vsRange.addHand(new Uint8Array([43, 42, 39, 38]), 1.0) // QQJJ

calculator.setOmahaHeroRange(heroRange)
calculator.setOmahaVsRange(vsRange)

const flop = new Uint8Array([0, 1, 2]) // 2s 2h 2d

// Equity for every hero hand vs the villain range, aggregated over runouts.
// Pass 1000 to Monte Carlo sample the turn/river instead of full enumeration.
const results = calculator.omahaEquityVsRange(flop, 1000)

results.forEach((result) => {
  const { hand, equity } = result
  const total = equity.win + equity.tie + equity.lose
  console.log(`Hand: [${hand}]`)
  console.log(`  Win: ${(equity.win / total).toFixed(3)}`)
  console.log(`  Tie: ${(equity.tie / total).toFixed(3)}`)
  console.log(`  Lose: ${(equity.lose / total).toFixed(3)}`)
})
```
