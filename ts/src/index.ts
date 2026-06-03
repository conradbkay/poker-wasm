import {
  HoldemRange,
  EquityCalculator,
  OmahaRange
} from "../../pkg/poker_wasm.js"
// import { HoldemRange, EquityCalculator } from "poker-wasm"
import { formatCards } from "poker-utils"

function createFullRange() {
  const range = new HoldemRange()
  for (let i = 0; i < 1326; i++) {
    range.set(i, 1)
  }
  return range
}

async function main() {
  try {
    const calculator = new EquityCalculator()

    const board = [0, 6, 12, 19, 43]
    const vsRange = createFullRange()
    const myRange = createFullRange()

    calculator.setHeroRange(myRange)
    calculator.setVsRange(vsRange)

    const equityResults = calculator.equityVsRange(new Uint8Array(board))

    console.log(
      `Equity calculation completed with ${
        equityResults.length
      } results on board ${formatCards(board)}`
    )
    ;[...equityResults.slice(0, 40), ...equityResults.slice(-40)].forEach(
      (result, i) => {
        const equity = result.equity
        console.log(
          `Result ${i}: Hand ${formatCards(
            Array.from(result.combo)
          )} - Win: ${equity.win.toFixed(3)}, Tie: ${equity.tie.toFixed(
            3
          )}, Lose: ${equity.lose.toFixed(3)}`
        )
      }
    )

    // Test PLO4 (4-card Omaha) — Monte Carlo turn/river sampling on the flop
    console.log("\nTesting PLO4 Monte Carlo (1000 runouts)...")
    const plo4Hero = new OmahaRange(4)
    plo4Hero.addHand(new Uint8Array([35, 34, 31, 30]), 1.0)
    const plo4Vs = new OmahaRange(4)
    plo4Vs.addHand(new Uint8Array([51, 50, 47, 46]), 1.0) // AAKK
    plo4Vs.addHand(new Uint8Array([43, 42, 39, 38]), 1.0) // QQJJ
    calculator.setOmahaHeroRange(plo4Hero)
    calculator.setOmahaVsRange(plo4Vs)

    const plo4Result = calculator.omahaEquityVsRange(
      new Uint8Array([0, 6, 12]), // flop
      1000 // sampled runouts
    )
    console.log(`PLO4 Monte Carlo completed: ${plo4Result.length} hero result(s)`)

    // Test PLO5 (5-card Omaha) — full 5-card board, no enumeration
    console.log("\nTesting PLO5 equity calculation...")
    const plo5Hero = new OmahaRange(5)
    plo5Hero.addHand(new Uint8Array([35, 34, 31, 30, 28]), 1.0) // Hero: TT99J
    const plo5Vs = new OmahaRange(5)
    plo5Vs.addHand(new Uint8Array([51, 50, 47, 46, 44]), 1.0) // AAKKQ
    plo5Vs.addHand(new Uint8Array([43, 42, 39, 38, 36]), 1.0) // QQJJT
    calculator.setOmahaHeroRange(plo5Hero)
    calculator.setOmahaVsRange(plo5Vs)

    const plo5Equity = calculator.omahaEquityVsRange(
      new Uint8Array([0, 6, 12, 19, 43])
    )[0].equity
    console.log(
      `PLO5 equity - Win: ${plo5Equity.win.toFixed(3)}, ` +
        `Tie: ${plo5Equity.tie.toFixed(3)}, ` +
        `Lose: ${plo5Equity.lose.toFixed(3)}`
    )

    // Test PLO6 (6-card Omaha) — full 5-card board, no enumeration
    console.log("\nTesting PLO6 equity calculation...")
    const plo6Hero = new OmahaRange(6)
    plo6Hero.addHand(new Uint8Array([35, 33, 31, 30, 28, 26]), 1.0) // Hero: TT99JJ
    const plo6Vs = new OmahaRange(6)
    plo6Vs.addHand(new Uint8Array([51, 50, 47, 46, 44, 42]), 1.0) // AAKKQJ
    plo6Vs.addHand(new Uint8Array([43, 41, 39, 38, 36, 34]), 1.0) // QQJJTT
    calculator.setOmahaHeroRange(plo6Hero)
    calculator.setOmahaVsRange(plo6Vs)

    const plo6Equity = calculator.omahaEquityVsRange(
      new Uint8Array([0, 6, 12, 19, 43])
    )[0].equity
    console.log(
      `PLO6 equity - Win: ${plo6Equity.win.toFixed(3)}, ` +
        `Tie: ${plo6Equity.tie.toFixed(3)}, ` +
        `Lose: ${plo6Equity.lose.toFixed(3)}`
    )

    console.log("\nAll Omaha tests (PLO4, PLO5, PLO6) completed successfully!")
  } catch (error) {
    console.error("An error occurred during the test run:", error)
  }
}

main()
