import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const packageDir = dirname(dirname(fileURLToPath(import.meta.url)))
const workspaceDir = join(packageDir, '..', '..')
const typeFolder = join(workspaceDir, 'target', 'napi-types')
await mkdir(typeFolder, { recursive: true })
const cargo = spawnSync(
  'cargo',
  ['build', '--manifest-path', join(workspaceDir, 'Cargo.toml'), '--package', 'xcss-node'],
  { cwd: workspaceDir, encoding: 'utf8', env: { ...process.env, NAPI_TYPE_DEF_TMP_FOLDER: typeFolder }, shell: process.platform === 'win32' },
)

if (cargo.stdout) process.stdout.write(cargo.stdout)
if (cargo.stderr) process.stderr.write(cargo.stderr)
if (cargo.status !== 0) process.exit(cargo.status ?? 1)
const definitions = (await readFile(join(typeFolder, 'xcss-node'), 'utf8')).trim().split('\n').map(line => JSON.parse(line))
const classes = new Map()
const declarations = []
for (const definition of definitions) {
  if (definition.kind === 'impl') {
    classes.set(definition.name, [...(classes.get(definition.name) ?? []), definition.def])
  } else if (definition.kind === 'struct') {
    classes.set(definition.name, [...(classes.get(definition.name) ?? []), definition.def])
  } else {
    declarations.push(definition.kind === 'interface'
      ? `export interface ${definition.name} {\n${definition.def}\n}`
      : `export declare ${definition.def}`)
  }
}
for (const [name, members] of classes) declarations.push(`export declare class ${name} {\n${members.join('\n')}\n}`)
await writeFile(join(packageDir, 'index.d.ts'), `// Generated from napi-rs binding metadata. Do not edit by hand.\n${declarations.join('\n\n')}\n`)

const libraryName = process.platform === 'win32'
  ? 'xcss_node.dll'
  : process.platform === 'darwin'
    ? 'libxcss_node.dylib'
    : 'libxcss_node.so'

await copyFile(join(workspaceDir, 'target', 'debug', libraryName), join(packageDir, 'xcss.node'))
