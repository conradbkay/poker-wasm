# power-range-rs

This is a rust port of the algorithm in <https://github.com/conradbkay/poker-utils> for calculating the equity of a Hold'em range vs a second range

## Benchmarks

The benchmark randomizes the board and both ranges for each trial

![](assets/lines.svg)

For hand evaluation it's using the twoplustwo algorithm/lookup table, but it only evaluates the union of both range's (so a max of 1326 evaluations), so even swapping in a 10x slower evaluator wouldn't harm performance much.

The original Typescript implementation has tests. This module should be tested by comparing it's output to the Typescript implementation's results, probably from an exported JSON file
