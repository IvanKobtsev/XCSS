#!/usr/bin/env node

import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const version = JSON.parse(await readFile(join(root, 'package.json'), 'utf8')).version
const libc = process.platform === 'linux'
  ? process.report?.getReport?.().header?.glibcVersionRuntime ? 'gnu' : 'musl'
  : ''
const platformKey = `${process.platform}-${process.arch}${libc ? `-${libc}` : ''}`
const platformNames = {
  'darwin-arm64': '@cross-css/native-darwin-arm64',
  'darwin-x64': '@cross-css/native-darwin-x64',
  'linux-arm64-gnu': '@cross-css/native-linux-arm64-gnu',
  'linux-arm64-musl': '@cross-css/native-linux-arm64-musl',
  'linux-x64-gnu': '@cross-css/native-linux-x64-gnu',
  'linux-x64-musl': '@cross-css/native-linux-x64-musl',
  'win32-arm64': '@cross-css/native-win32-arm64-msvc',
  'win32-x64': '@cross-css/native-win32-x64-msvc',
}
const packageName = platformNames[platformKey]
if (!packageName) throw new Error(`unsupported smoke-test platform ${platformKey}`)

const testRoot = await mkdtemp(join(tmpdir(), 'xcss-release-'))
const platformDir = join(testRoot, 'platform')
const tarballDir = join(testRoot, 'tarballs')
await mkdir(tarballDir, { recursive: true })

function run(command, args, cwd = root) {
  const useShell = process.platform === 'win32' && command === 'npm'
  const env = { ...process.env }
  delete env.npm_config_verify_deps_before_run
  delete env.npm_config__jsr_registry
  const result = spawnSync(command, args, {
    cwd,
    encoding: 'utf8',
    env,
    shell: useShell,
  })
  if (result.stdout) process.stdout.write(result.stdout)
  if (result.stderr) process.stderr.write(result.stderr)
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}

try {
  run(process.execPath, [
    join(root, 'npm', 'create-platform-package.mjs'),
    join(root, 'packages', 'native', 'xcss.node'),
    platformDir,
    packageName,
    process.platform,
    process.arch,
    libc,
  ])
  run('npm', ['pack', platformDir, '--pack-destination', tarballDir])

  const platformTarName = packageName.replace('@cross-css/', 'cross-css-').replaceAll('/', '-')
  const tarballs = [
    join(root, 'dist', 'main-packages', `cross-css-native-${version}.tgz`),
    join(root, 'dist', 'main-packages', `cross-css-vite-plugin-${version}.tgz`),
    join(tarballDir, `${platformTarName}-${version}.tgz`),
  ]
  run('npm', ['install', '--ignore-scripts', '--no-package-lock', ...tarballs], testRoot)

  run(process.execPath, [
    '--input-type=module',
    '--eval',
    `import { createRequire } from 'node:module';
     import { join } from 'node:path';
     import { pathToFileURL } from 'node:url';
     const root = process.argv[1];
     const requireFromTest = createRequire(join(root, 'entry.js'));
     const source = 'a { width: --size * 2; }';
     const result = requireFromTest('@cross-css/native').transform(
       [{ moduleId: 'a.xcss', filename: '/project/a.xcss', source }], [], 'a.xcss', '/project/node_modules/.xcss/_injections.scss');
     if (!result.code?.endsWith('a { width: calc(var(--size) * 2); }')) throw new Error(JSON.stringify(result));
     const pluginUrl = pathToFileURL(join(root, 'node_modules', '@cross-css', 'vite-plugin', 'dist', 'index.js'));
     const plugin = (await import(pluginUrl.href)).default();
     if (plugin.name !== 'cross-css' || plugin.enforce !== 'pre') throw new Error('packed Vite plugin did not load correctly');`,
    testRoot,
  ], testRoot)
  console.log(`packed release smoke test passed on ${platformKey}`)
} finally {
  await rm(testRoot, { recursive: true, force: true })
}
