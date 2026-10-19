#!/usr/bin/env node

import { mkdir } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const destination = join(root, 'dist', 'main-packages')
await mkdir(destination, { recursive: true })

for (const packageName of ['@cross-css/native', '@cross-css/vite-plugin']) {
  const packed = spawnSync(
    'corepack',
    ['pnpm', '--filter', packageName, 'pack', '--pack-destination', destination],
    { cwd: root, encoding: 'utf8', shell: process.platform === 'win32' },
  )
  if (packed.stdout) process.stdout.write(packed.stdout)
  if (packed.stderr) process.stderr.write(packed.stderr)
  if (packed.status !== 0) process.exit(packed.status ?? 1)
}
