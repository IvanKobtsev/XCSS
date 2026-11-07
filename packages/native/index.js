'use strict'

const { existsSync } = require('node:fs')
const { join } = require('node:path')

const target = require('./platform.cjs')

const localBinding = join(__dirname, 'xcss.node')
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

module.exports.formatSource = binding.formatSource
