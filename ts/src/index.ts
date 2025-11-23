import { HoldemRange, EquityCalculator, OmahaRange } from "../../pkg/poker_wasm.js"
// import { HoldemRange, EquityCalculator } from "poker-wasm"
import fs from "fs"
import path from "path"
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
    const handRanksPath = path.join(__dirname, "..", "..", "HandRanks.dat")
    const handRanksData = fs.readFileSync(handRanksPath)

    const calculator = new EquityCalculator(handRanksData)

    const board = [0, 6, 12, 19, 43]
    const vsRange = createFullRange()
    const myRange = createFullRange()

    calculator.setHeroRange(myRange)
    calculator.setVsRange(vsRange)

    const equityResults = calculator.equity_vs_range(new Uint8Array(board))

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

    // Test PLO4 (4-card Omaha)
    console.log('\nTesting PLO4 Monte Carlo (100 iterations)...')
    const plo4Range = new OmahaRange(4)
    plo4Range.addHand(new Uint8Array([51, 50, 47, 46]), 1.0) // AAKK
    plo4Range.addHand(new Uint8Array([43, 42, 39, 38]), 1.0) // QQJJ
    calculator.setOmahaRange(plo4Range)

    for (let i = 0; i < 100; i++) {
      const result = calculator.omahaMonteCarloFlop(
        new Uint8Array([35, 34, 31, 30]),
        new Uint8Array([0, 6, 12]),
        1000
      )
      if (i === 0) {
        console.log(`Sample PLO4 result (iteration 0): ${result.length} runouts`)
      }
    }
    console.log('PLO4 Monte Carlo test completed successfully!')

    // Test PLO5 (5-card Omaha)
    console.log('\nTesting PLO5 equity calculation...')
    const plo5Range = new OmahaRange(5)
    plo5Range.addHand(new Uint8Array([51, 50, 47, 46, 44]), 1.0) // AAKKQ
    plo5Range.addHand(new Uint8Array([43, 42, 39, 38, 36]), 1.0) // QQJJT
    calculator.setOmahaRange(plo5Range)

    const plo5Result = calculator.omahaLeafEquityVsRange(
      new Uint8Array([35, 34, 31, 30, 28]), // Hero: TT99J
      new Uint8Array([0, 6, 12, 19, 43])
    )
    console.log(`PLO5 equity - Win: ${plo5Result.equity.win.toFixed(3)}, ` +
                `Tie: ${plo5Result.equity.tie.toFixed(3)}, ` +
                `Lose: ${plo5Result.equity.lose.toFixed(3)}`)

    // Test PLO6 (6-card Omaha)
    console.log('\nTesting PLO6 equity calculation...')
    const plo6Range = new OmahaRange(6)
    plo6Range.addHand(new Uint8Array([51, 50, 47, 46, 44, 42]), 1.0) // AAKKQJ
    plo6Range.addHand(new Uint8Array([43, 41, 39, 38, 36, 34]), 1.0) // QQJJTT
    calculator.setOmahaRange(plo6Range)

    const plo6Result = calculator.omahaLeafEquityVsRange(
      new Uint8Array([35, 33, 31, 30, 28, 26]), // Hero: TT99JJ
      new Uint8Array([0, 6, 12, 19, 43])
    )
    console.log(`PLO6 equity - Win: ${plo6Result.equity.win.toFixed(3)}, ` +
                `Tie: ${plo6Result.equity.tie.toFixed(3)}, ` +
                `Lose: ${plo6Result.equity.lose.toFixed(3)}`)

    console.log('\nAll Omaha tests (PLO4, PLO5, PLO6) completed successfully!')
  } catch (error) {
    console.error("An error occurred during the test run:", error)
  }
}

main()
