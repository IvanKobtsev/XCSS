'use strict'

const { join } = require('node:path')

const libc = process.platform === 'linux'
  ? process.report?.getReport?.().header?.glibcVersionRuntime ? 'gnu' : 'musl'
  : null
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

const executable = process.platform === 'win32' ? 'xcss.exe' : 'xcss'
module.exports = { key, name: packages[key], cliPath: join('bin', executable) }
