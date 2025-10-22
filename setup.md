# Setup

The original Typescript implementation has tests. This module should be tested by comparing it's output to the Typescript implementation's results, probably from an exported JSON file

first add a HandRanks.dat file to the project root, which you can download here <https://github.com/chenosaurus/poker-evaluator/blob/master/data/HandRanks.dat>

Simply run npm i then npm run main in the `ts` dir

## Build

wasm-pack build --target nodejs
