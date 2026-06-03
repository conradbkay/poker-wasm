# power-range-rs

Hold'em range vs range equity calculation in 11 µs or less

## Usage

```bash
npm install poker-wasm
# optionally https://github.com/conradbkay/poker-utils
npm install poker-utils
```

### NLHE Equity Calculation

Calculate equity for a Hold'em range vs another range:

```ts
import { EquityCalculator, HoldemRange } from "poker-wasm"

const calculator = new EquityCalculator()

const heroRange = new HoldemRange()
const vsRange = new HoldemRange()

// Add hands to ranges (cards are 0-51, As=51)
heroRange.setHand(new Uint8Array([51, 50]), 1.0) // AA with 100% weight
heroRange.setHand(new Uint8Array([47, 46]), 1.0) // KK with 100% weight

vsRange.setHand(new Uint8Array([43, 42]), 1.0) // QQ
vsRange.setHand(new Uint8Array([39, 38]), 1.0) // JJ

// Set ranges once (avoids repeated memory transfers)
calculator.setHeroRange(heroRange)
calculator.setVsRange(vsRange)

// Calculate equity on a flop
const board = new Uint8Array([0, 12, 28])
const results = calculator.equityVsRange(board)

// Results contain equity for each hand in hero's range
results.forEach(({ combo, equity }) => {
  console.log(
    `[${combo}] 
    win=${equity.win.toFixed(3)}
    tie=${equity.tie.toFixed(3)}
    lose=${equity.lose.toFixed(3)}`
  )
})
```

### Omaha (PLO) Range vs Range Equity

Calculate PLO equity for a hero range vs a villain range. The board may be 3, 4,
or 5 cards; incomplete boards are enumerated. On a flop, passing `maxRunouts`
samples that many turn/river runouts (Monte Carlo) instead of enumerating all of
them. Works for PLO4, PLO5, and PLO6 (the range's hand size sets the variant).

```ts
import { EquityCalculator, OmahaRange } from "poker-wasm"

const calculator = new EquityCalculator()

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
// ^ same format/type as results from NLHE example
```
