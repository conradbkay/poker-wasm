import rvr from "../../pkg/poker_wasm.js"
import fs from "fs"
import path from "path"
import { formatCards } from "poker-utils"

function createRandomRange(board: number[], numIterations: number) {
  const range = new rvr.HoldemRange()
  for (let i = 0; i < numIterations; i++) {
    let card1 = Math.floor(Math.random() * 52)
    let card2 = Math.floor(Math.random() * 52)

    while (card2 === card1) {
      card2 = Math.floor(Math.random() * 52)
    }

    if (board.includes(card1) || board.includes(card2)) {
      continue
    }

    const weight = 1
    const hand = new Uint8Array([card1, card2])

    try {
      range.set_hand(hand, weight)
    } catch (e) {
      console.error(`Error setting hand ${hand}: ${e}`)
    }
  }
  return range
}

async function main() {
  try {
    const handRanksPath = path.join(__dirname, "..", "..", "HandRanks.dat")
    const handRanksData = fs.readFileSync(handRanksPath)

    const calculator = new rvr.EquityCalculator(handRanksData)

    const board = [0, 1, 2, 3, 4]
    const vsRange = createRandomRange(board, 100000)
    const myRange = createRandomRange(board, 100000)

    console.log(
      `${myRange.get_range().filter((w) => w > 0).length} vs ${
        vsRange.get_range().filter((w) => w > 0).length
      } hands with non-zero weights`
    )

    const equityResults = calculator.equity_vs_range(
      myRange,
      vsRange,
      new Uint8Array(board)
    )

    console.log(
      `Equity calculation completed with ${
        equityResults.length
      } results on board ${formatCards(board)}`
    )

    equityResults.slice(0, 50).forEach((result, i) => {
      const equity = result.equity
      console.log(
        `Result ${i}: Hand ${formatCards(
          Array.from(result.combo)
        )} - Win: ${equity.win.toFixed(3)}, Tie: ${equity.tie.toFixed(
          3
        )}, Lose: ${equity.lose.toFixed(3)}`
      )
    })
  } catch (error) {
    console.error("An error occurred during the test run:", error)
  }
}

main()
