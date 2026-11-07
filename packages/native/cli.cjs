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

    const result = spawnSync(executable, args, { stdio: 'inherit' })
    if (result.error) throw result.error
    return result.status ?? 1
  } catch (error) {
    console.error(`Unable to run the XCSS CLI (${name ?? key}): ${error.message}`)
    return 1
  }
}
