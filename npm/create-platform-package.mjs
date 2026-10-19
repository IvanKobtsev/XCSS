#!/usr/bin/env node

import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const [source, output, packageName, os, cpu, libc = ''] = process.argv.slice(2)
if (!source || !output || !packageName || !os || !cpu) {
  console.error('usage: create-platform-package <binding> <output> <package-name> <os> <cpu> [libc]')
  process.exit(2)
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const rootPackage = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'))
const outputDir = resolve(output)
await mkdir(outputDir, { recursive: true })
await copyFile(resolve(source), join(outputDir, 'xcss.node'))

const manifest = {
  name: packageName,
  version: rootPackage.version,
  description: `Precompiled XCSS native binding for ${os}/${cpu}${libc ? `/${libc}` : ''}`,
  license: 'MIT',
  author: 'Ivan Kobtsev',
  repository: {
    type: 'git',
    url: 'git+https://github.com/IvanKobtsev/XCSS.git',
  },
  main: 'xcss.node',
  files: ['xcss.node', 'README.md', 'LICENSE'],
  os: [os],
  cpu: [cpu],
  engines: { node: '^20.19.0 || >=22.12.0' },
  publishConfig: { access: 'public' },
}
if (libc) manifest.libc = [libc === 'gnu' ? 'glibc' : libc]

await writeFile(join(outputDir, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`)
await copyFile(join(root, 'packages', 'native', 'README.md'), join(outputDir, 'README.md'))
await copyFile(join(root, 'LICENSE'), join(outputDir, 'LICENSE'))
