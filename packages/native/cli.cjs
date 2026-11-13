'use strict'

const { spawnSync } = require('node:child_process')
const { existsSync } = require('node:fs')
const { dirname, join } = require('node:path')
const { cliPath, key, name } = require('./platform.cjs')

module.exports = function runCli(args) {
  try {
    let executable = join(__dirname, cliPath)
    if (!existsSync(executable)) {
      if (!name) throw new Error(`No XCSS CLI is available for ${key}.`)
      const manifest = require.resolve(`${name}/package.json`)
      executable = join(dirname(manifest), cliPath)
    }

    const checking = args[0] === 'check'
    const result = spawnSync(executable, checking ? [...args, '--warning-inputs'] : args, {
      stdio: checking ? ['inherit', 'pipe', 'inherit'] : 'inherit', encoding: 'utf8',
    })
    if (checking && result.stdout?.trim()) {
      const { validateCss } = require('./css-validation.cjs')
      const warnings = JSON.parse(result.stdout).flatMap(input =>
        [...input.warnings, ...validateCss(input)].map(warning => ({ ...warning, file: input.file })))
      if (warnings.length) {
        console.error('Warnings:')
        for (const warning of warnings) console.error(`${warning.file}:${warning.range.start.line + 1}:${warning.range.start.character + 1}: Warning: ${warning.message}`)
      }
    }
    if (result.error) throw result.error
    return result.status ?? 1
  } catch (error) {
    console.error(`Unable to run the XCSS CLI (${name ?? key}): ${error.message}`)
    return 1
  }
}
