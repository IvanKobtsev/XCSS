#!/usr/bin/env node

import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
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

function run(command, args, cwd = root, expectedStatus = 0) {
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
  if (result.status !== expectedStatus) {
    throw new Error(`${command} exited with status ${result.status}, expected ${expectedStatus}`)
  }
  return result
}

try {
  run(process.execPath, [
    join(root, 'npm', 'create-platform-package.mjs'),
    join(root, 'packages', 'native', 'xcss.node'),
    join(root, 'packages', 'native', 'bin', process.platform === 'win32' ? 'xcss.exe' : 'xcss'),
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
    join(root, 'dist', 'main-packages', `cross-css-vite-${version}.tgz`),
    join(tarballDir, `${platformTarName}-${version}.tgz`),
  ]
  const manifestPath = join(testRoot, 'package.json')
  await writeFile(manifestPath, JSON.stringify({ private: true }))
  run('npm', ['install', '--ignore-scripts', '--no-package-lock', ...tarballs], testRoot)

  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'))
  manifest.scripts = { 'check:xcss': 'xcss check --project tsconfig.app.json', 'check:stylesheets': 'xcss check' }
  await writeFile(manifestPath, JSON.stringify(manifest))
  // pnpm injects npm_config_* settings that npm may warn about. Keep the
  // script's stderr intact so the assertions only see XCSS output.
  const runCheck = (script, expectedStatus = 0) =>
    run('npm', ['--loglevel=error', 'run', script], testRoot, expectedStatus)
  await writeFile(join(testRoot, 'tsconfig.app.json'), JSON.stringify({
    compilerOptions: { module: 'esnext', moduleResolution: 'bundler', types: ['@cross-css/vite/client'] },
    files: ['app.ts'],
  }))
  await writeFile(join(testRoot, 'button.xcss'), '.button {}')
  const appPath = join(testRoot, 'app.ts')
  const source = "import { styles } from './button.xcss'; import './unrelated'; const count: number = 'bad';\n"
  await writeFile(appPath, source + "styles.button;\n")
  const success = runCheck('check:xcss')
  if (success.stderr.trim()) throw new Error(`Unexpected stderr from the packaged XCSS CLI:\n${success.stderr}`)
  await writeFile(appPath, source + "styles.missing;\n")
  const failure = runCheck('check:xcss', 1)
  if (!failure.stderr.includes("XC-001 Unknown XCSS class — Class 'missing' can't be found in 'button.xcss'.")
      || failure.stderr.includes('unrelated') || /\bTS\d+\b/.test(failure.stderr)) {
    throw new Error('The packaged XCSS CLI did not isolate the missing class')
  }
  await writeFile(join(testRoot, 'tsconfig.app.json'), '{ invalid:')
  const warning = runCheck('check:xcss')
  if (!warning.stderr.startsWith("Warning: Couldn't analyze XCSS TypeScript integration: ")
      || warning.stderr.trim().split('\n').length !== 1) {
    throw new Error('The packaged XCSS CLI did not report a standalone setup warning')
  }
  await writeFile(join(testRoot, 'button.xcss'), 'bogus;')
  for (const [script, expectsWarning] of [['check:stylesheets', false], ['check:xcss', true]]) {
    const stylesheetFailure = runCheck(script, 1)
    if (!stylesheetFailure.stderr.includes('XC-105 Invalid XCSS statement')
        || stylesheetFailure.stderr.includes('Warning:') !== expectsWarning) {
      throw new Error('The packaged XCSS CLI did not check stylesheets independently')
    }
  }

  run(process.execPath, [
    '--input-type=module',
    '--eval',
    `import { createRequire } from 'node:module';
     import { join } from 'node:path';
     const root = process.argv[1];
     const requireFromTest = createRequire(join(root, 'entry.js'));
     const source = 'a { width: --size * 2; }';
     const formatted = requireFromTest('@cross-css/native').formatSource('.a{color:red}');
     if (formatted.text !== '.a {\\n  color: red;\\n}\\n') throw new Error('packed formatter did not load');
     const compiler = new (requireFromTest('@cross-css/native').Compiler)();
     compiler.setFile('a.xcss', 1, source);
     const result = compiler.compile('a.xcss');
     if (!result.success || !result.artifacts[0].css.includes('calc(var(--size) * 2)')) throw new Error(JSON.stringify(result));
     const plugin = (await import('@cross-css/vite')).default();
     if (plugin.name !== 'cross-css' || plugin.enforce !== 'pre') throw new Error('packed Vite plugin did not load correctly');`,
    testRoot,
  ], testRoot)
  run(process.execPath, [join(root, 'npm', 'test-published-language-service.mjs')], testRoot)
  console.log(`packed release smoke test passed on ${platformKey}`)
} finally {
  await rm(testRoot, { recursive: true, force: true })
}
