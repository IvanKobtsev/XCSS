#!/usr/bin/env node

import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const readJson = async (path) => JSON.parse(await readFile(path, 'utf8'))
const rootPackage = await readJson(join(root, 'package.json'))
const nativePackage = await readJson(join(root, 'packages', 'native', 'package.json'))
const vitePackage = await readJson(join(root, 'packages', 'vite-plugin', 'package.json'))
const cargoToml = await readFile(join(root, 'Cargo.toml'), 'utf8')
const cargoLock = await readFile(join(root, 'Cargo.lock'), 'utf8')
const cargoVersion = cargoToml.match(/\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)?.[1]
const nodeLockVersion = cargoLock.match(/\[\[package\]\]\s+name = "xcss-node"\s+version = "([^"]+)"/)?.[1]
const version = rootPackage.version

const versions = {
  'root package': version,
  '@cross-css/native': nativePackage.version,
  '@cross-css/vite-plugin': vitePackage.version,
  'Cargo workspace': cargoVersion,
  'Cargo.lock xcss-node': nodeLockVersion,
}

for (const [name, actual] of Object.entries(versions)) {
  if (actual !== version) throw new Error(`${name} version ${actual ?? '<missing>'} does not match ${version}`)
}

const expectedPlatforms = new Set([
  '@cross-css/native-darwin-arm64',
  '@cross-css/native-darwin-x64',
  '@cross-css/native-linux-arm64-gnu',
  '@cross-css/native-linux-arm64-musl',
  '@cross-css/native-linux-x64-gnu',
  '@cross-css/native-linux-x64-musl',
  '@cross-css/native-win32-arm64-msvc',
  '@cross-css/native-win32-x64-msvc',
])

const optionalDependencies = nativePackage.optionalDependencies ?? {}
if (Object.keys(optionalDependencies).length !== expectedPlatforms.size) {
  throw new Error(`expected ${expectedPlatforms.size} native platform dependencies`)
}
for (const [name, dependencyVersion] of Object.entries(optionalDependencies)) {
  if (!expectedPlatforms.delete(name) || dependencyVersion !== version) {
    throw new Error(`invalid native platform dependency ${name}@${dependencyVersion}`)
  }
}
const platformRoot = join(root, 'packages', 'native-platforms')
for (const name of Object.keys(optionalDependencies)) {
  const directory = name.replace('@cross-css/native-', '')
  const platformPackage = await readJson(join(platformRoot, directory, 'package.json'))
  if (platformPackage.name !== name || platformPackage.version !== version || platformPackage.private !== true) {
    throw new Error(`invalid workspace stub for ${name}`)
  }
}
if (vitePackage.dependencies?.['@cross-css/native'] !== version) {
  throw new Error('@cross-css/vite-plugin must depend on the matching @cross-css/native version')
}

console.log(`release metadata is consistent for ${version}`)
