#!/usr/bin/env node

import { chmod, copyFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const [source, cli, output, packageName, os, cpu, libc = ''] = process.argv.slice(2)
if (!source || !cli || !output || !packageName || !os || !cpu) {
  console.error('usage: create-platform-package <binding> <cli> <output> <package-name> <os> <cpu> [libc]')
  process.exit(2)
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const rootPackage = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'))
const outputDir = resolve(output)
await mkdir(outputDir, { recursive: true })
await copyFile(resolve(source), join(outputDir, 'xcss.node'))
const cliFile = os === 'win32' ? 'bin/xcss.exe' : 'bin/xcss'
await mkdir(join(outputDir, 'bin'), { recursive: true })
await copyFile(resolve(cli), join(outputDir, cliFile))
if (os !== 'win32') await chmod(join(outputDir, cliFile), 0o755)

const manifest = {
  name: packageName,
  version: rootPackage.version,
  description: `Precompiled XCSS native binding and CLI for ${os}/${cpu}${libc ? `/${libc}` : ''}`,
  license: 'MIT',
  author: 'Ivan Kobtsev',
  repository: {
    type: 'git',
    url: 'git+https://github.com/IvanKobtsev/XCSS.git',
  },
  main: 'xcss.node',
  files: ['xcss.node', cliFile, 'README.md', 'LICENSE', 'TSRS-LICENSE', 'TSRS-NOTICE'],
  os: [os],
  cpu: [cpu],
  engines: { node: '^20.19.0 || >=22.12.0' },
  publishConfig: { access: 'public' },
}
if (libc) manifest.libc = [libc === 'gnu' ? 'glibc' : libc]

await writeFile(join(outputDir, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`)
await copyFile(join(root, 'packages', 'native', 'README.md'), join(outputDir, 'README.md'))
await copyFile(join(root, 'LICENSE'), join(outputDir, 'LICENSE'))
await copyFile(join(root, 'packages', 'native', 'TSRS-LICENSE'), join(outputDir, 'TSRS-LICENSE'))
await copyFile(join(root, 'packages', 'native', 'TSRS-NOTICE'), join(outputDir, 'TSRS-NOTICE'))
