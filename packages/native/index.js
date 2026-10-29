'use strict'

const { existsSync } = require('node:fs')
const { join } = require('node:path')

function linuxLibc() {
  if (process.platform !== 'linux') return null
  const report = process.report?.getReport?.()
  return report?.header?.glibcVersionRuntime ? 'gnu' : 'musl'
}

function platformPackage() {
  const libc = linuxLibc()
  const key = `${process.platform}-${process.arch}${libc ? `-${libc}` : ''}`
  const packages = {
    'darwin-arm64': '@cross-css/native-darwin-arm64',
    'darwin-x64': '@cross-css/native-darwin-x64',
    'linux-arm64-gnu': '@cross-css/native-linux-arm64-gnu',
    'linux-arm64-musl': '@cross-css/native-linux-arm64-musl',
    'linux-x64-gnu': '@cross-css/native-linux-x64-gnu',
    'linux-x64-musl': '@cross-css/native-linux-x64-musl',
    'win32-arm64': '@cross-css/native-win32-arm64-msvc',
    'win32-x64': '@cross-css/native-win32-x64-msvc',
  }
  return { key, name: packages[key] }
}

const localBinding = join(__dirname, 'xcss.node')
const target = platformPackage()
let binding

try {
  binding = existsSync(localBinding) ? require(localBinding) : require(target.name)
} catch (error) {
  const supported = target.name ? target.name : `no package is published for ${target.key}`
  const detail = error instanceof Error ? `\n${error.message}` : ''
  throw new Error(`Unable to load the XCSS native binding (${supported}).${detail}`, {
    cause: error,
  })
}

module.exports = binding
module.exports.Compiler = binding.Compiler
module.exports.inspect = binding.inspect
module.exports.LanguageService = binding.LanguageService
module.exports.languageServiceCapabilities = binding.languageServiceCapabilities
