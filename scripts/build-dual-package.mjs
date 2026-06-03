#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync
} from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const rootDir = dirname(dirname(fileURLToPath(import.meta.url)))
const pkgDir = join(rootDir, "pkg")
const nodePkgDir = mkdtempSync(join(tmpdir(), "poker-wasm-node-"))

function wasmPackBuild(target, outDir) {
  execFileSync("wasm-pack", ["build", "--target", target, "--out-dir", outDir], {
    cwd: rootDir,
    stdio: "inherit"
  })
}

try {
  wasmPackBuild("bundler", "pkg")
  wasmPackBuild("nodejs", nodePkgDir)

  const cjsSource = readFileSync(join(nodePkgDir, "poker_wasm.js"), "utf8")
  if (!cjsSource.includes("poker_wasm_bg.wasm")) {
    throw new Error("Node wasm-pack wrapper did not reference poker_wasm_bg.wasm")
  }

  const cjsWrapper = cjsSource.replaceAll(
    "poker_wasm_bg.wasm",
    "poker_wasm_node_bg.wasm"
  )

  writeFileSync(join(pkgDir, "poker_wasm.cjs"), cjsWrapper)
  copyFileSync(
    join(nodePkgDir, "poker_wasm_bg.wasm"),
    join(pkgDir, "poker_wasm_node_bg.wasm")
  )

  const packageJsonPath = join(pkgDir, "package.json")
  const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"))

  packageJson.main = "./poker_wasm.cjs"
  packageJson.module = "./poker_wasm.js"
  packageJson.types = "./poker_wasm.d.ts"
  packageJson.exports = {
    ".": {
      types: "./poker_wasm.d.ts",
      import: "./poker_wasm.js",
      require: "./poker_wasm.cjs"
    }
  }
  packageJson.files = [
    "poker_wasm_bg.wasm",
    "poker_wasm_node_bg.wasm",
    "poker_wasm.js",
    "poker_wasm.cjs",
    "poker_wasm_bg.js",
    "poker_wasm.d.ts",
    "poker_wasm_bg.wasm.d.ts"
  ]
  packageJson.sideEffects = [
    "./poker_wasm.js",
    "./poker_wasm.cjs",
    "./snippets/*"
  ]

  writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`)
} finally {
  rmSync(nodePkgDir, { force: true, recursive: true })
}
