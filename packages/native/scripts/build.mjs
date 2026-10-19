import { copyFile } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const packageDir = dirname(dirname(fileURLToPath(import.meta.url)))
const workspaceDir = join(packageDir, '..', '..')
const cargo = spawnSync(
  'cargo',
  ['build', '--manifest-path', join(workspaceDir, 'Cargo.toml'), '--package', 'xcss-node'],
  { cwd: workspaceDir, encoding: 'utf8', shell: process.platform === 'win32' },
)

if (cargo.stdout) process.stdout.write(cargo.stdout)
if (cargo.stderr) process.stderr.write(cargo.stderr)
if (cargo.status !== 0) process.exit(cargo.status ?? 1)

const libraryName = process.platform === 'win32'
  ? 'xcss_node.dll'
  : process.platform === 'darwin'
    ? 'libxcss_node.dylib'
    : 'libxcss_node.so'

await copyFile(join(workspaceDir, 'target', 'debug', libraryName), join(packageDir, 'xcss.node'))
