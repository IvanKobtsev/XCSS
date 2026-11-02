#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const allowed = new Map([
  ['xcss-source', new Set()],
  ['xcss-syntax', new Set(['xcss-source', 'raffia'])],
  ['xcss-analysis', new Set(['xcss-source', 'xcss-syntax'])],
  ['xcss-language-service', new Set(['xcss-source', 'xcss-syntax', 'xcss-analysis'])],
  ['xcss-codegen', new Set(['xcss-source', 'xcss-syntax', 'xcss-analysis', 'lightningcss', 'parcel_sourcemap', 'serde_json'])],
  ['xcss-core', new Set(['xcss-source', 'xcss-syntax', 'xcss-analysis', 'xcss-codegen'])],
])

function checkDependencies(name, dependencies) {
  for (const dependency of dependencies) {
    assert(allowed.get(name).has(dependency.name), `${name} cannot depend on ${dependency.name}`)
  }
}

function checkSource(source, path) {
  // Restrict both qualified access and grouped/aliased standard-library imports.
  const imports = [...source.matchAll(/\b(?:use|extern\s+crate)\s+std\b[^;]*;/g)].map(match => match[0])
  const direct = /\bstd\s*::\s*(?:fs|io|net|process|env)\b/
  const grouped = /\b(?:fs|io|net|process|env)\b/
  assert(!direct.test(source) && !imports.some(value => grouped.test(value)), `${path}: language engine must not access host I/O`)
}

// Negative controls ensure the checker actually rejects forbidden edges/access.
assert.throws(() => checkDependencies('xcss-analysis', [{ name: 'xcss-core' }]))
assert.throws(() => checkDependencies('xcss-source', [{ name: 'xcss-syntax' }]))
assert.throws(() => checkSource('use std::{sync::Arc, fs as disk};', 'negative control'))
assert.throws(() => checkSource('std::fs::read("file")', 'negative control'))

const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1', '--locked'], { cwd: root, encoding: 'utf8' }))
for (const [name] of allowed) {
  const pkg = metadata.packages.find(pkg => pkg.name === name)
  assert(pkg, `missing language crate ${name}`)
  checkDependencies(name, pkg.dependencies)
  function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) visit(path)
      else if (path.endsWith('.rs')) checkSource(readFileSync(path, 'utf8'), path)
    }
  }
  visit(join(dirname(pkg.manifest_path), 'src'))
}
console.log('language-engine dependency and host-I/O boundaries verified')
