# power-range-rs

This is a rust port of the algorithm in <https://github.com/conradbkay/poker-utils> for calculating the equity of a Hold'em range vs a second range

## Benchmarks

The benchmark randomizes the board and both ranges for each trial

![](assets/lines.svg)

For hand evaluation it's using the twoplustwo algorithm/lookup table, but it only evaluates the union of both range's (so a max of 1326 evaluations), so even swapping in a 10x slower evaluator wouldn't harm performance much.

## Usage

### Installing from npm

```bash
npm install poker-wasm
```

**Note:** This package is built with `--target nodejs` which automatically loads the WASM module. No manual initialization needed!

### NLHE Equity Calculation

Calculate equity for a Hold'em range vs another range:

```ts
import * as rvr from 'poker-wasm';
import fs from 'fs';

// Load the hand evaluation data file
const handRanksData = fs.readFileSync('./HandRanks.dat');
const calculator = new rvr.EquityCalculator(handRanksData);

// Create ranges
const heroRange = new rvr.HoldemRange();
const vsRange = new rvr.HoldemRange();

// Add hands to ranges (cards are 0-51)
// For example, AA = [51, 50], KK = [47, 46], etc.
heroRange.set_hand(new Uint8Array([51, 50]), 1.0); // AA with 100% weight
heroRange.set_hand(new Uint8Array([47, 46]), 1.0); // KK with 100% weight

vsRange.set_hand(new Uint8Array([43, 42]), 1.0); // QQ
vsRange.set_hand(new Uint8Array([39, 38]), 1.0); // JJ

// Calculate equity on a flop
const board = new Uint8Array([0, 12, 28]);
const results = calculator.equity_vs_range(heroRange, vsRange, board);

// Results contain equity for each hand in hero's range
results.forEach(result => {
  const { combo, equity } = result;
  console.log(`Hand: [${combo}]`);
  console.log(`  Win: ${equity.win.toFixed(3)}`);
  console.log(`  Tie: ${equity.tie.toFixed(3)}`);
  console.log(`  Lose: ${equity.lose.toFixed(3)}`);
});
```

### Omaha Monte Carlo Flop Equity

Calculate PLO equity using Monte Carlo simulation on the flop:

```ts
import * as rvr from 'poker-wasm';
import fs from 'fs';

// Load the hand evaluation data file
const handRanksData = fs.readFileSync('./HandRanks.dat');
const calculator = new rvr.EquityCalculator(handRanksData);

// Create an Omaha range (4 hole cards per hand)
const omahaRange = new rvr.OmahaRange();

// Add Omaha hands to the range
omahaRange.addHand(new Uint8Array([51, 50, 47, 46]), 1.0); // AAKK double suited
omahaRange.addHand(new Uint8Array([43, 42, 39, 38]), 1.0); // QQJJ

// Hero's hand (4 cards)
const heroHand = new Uint8Array([35, 34, 31, 30]); // TT99

// Flop (3 cards)
const flop = new Uint8Array([0, 1, 2]); // 2s 2h 2d

// Calculate equity using Monte Carlo with 1000 runouts
const numRunouts = 1000;
const runoutResults = calculator.omahaMonteCarloFlop(
  heroHand,
  omahaRange,
  flop,
  numRunouts
);

// Results contain equity for each sampled runout
console.log(`Sampled ${runoutResults.length} runouts`);
runoutResults.slice(0, 10).forEach((result, i) => {
  console.log(`Runout ${i}: Board [${result.board}]`);
  console.log(`  Win: ${result.equity.win.toFixed(3)}`);
  console.log(`  Tie: ${result.equity.tie.toFixed(3)}`);
  console.log(`  Lose: ${result.equity.lose.toFixed(3)}`);
});

// Calculate average equity across all runouts
const avgEquity = runoutResults.reduce(
  (acc, r) => ({
    win: acc.win + r.equity.win / runoutResults.length,
    tie: acc.tie + r.equity.tie / runoutResults.length,
    lose: acc.lose + r.equity.lose / runoutResults.length,
  }),
  { win: 0, tie: 0, lose: 0 }
);

console.log('\nAverage Equity:');
console.log(`  Win: ${avgEquity.win.toFixed(3)}`);
console.log(`  Tie: ${avgEquity.tie.toFixed(3)}`);
console.log(`  Lose: ${avgEquity.lose.toFixed(3)}`);
```
