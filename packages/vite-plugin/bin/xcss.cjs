#!/usr/bin/env node
'use strict'

const runCli = require('@cross-css/native/cli.cjs')
process.exitCode = runCli(process.argv.slice(2))
