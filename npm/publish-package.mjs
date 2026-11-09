import { spawnSync } from 'node:child_process'

const [tarball, packageId] = process.argv.slice(2)
const result = spawnSync('npm', ['publish', tarball, '--access', 'public', '--provenance', '--json'], {
  encoding: 'utf8',
  stdio: ['inherit', 'pipe', 'pipe'],
})
if (result.error) throw result.error

let errorCode
if (result.status !== 0) {
  try {
    errorCode = JSON.parse(result.stdout)?.error?.code
  } catch {
    // Keep npm's original output and failure status if it did not return JSON.
  }
}

if (errorCode === 'E409') {
  console.log(`Skipping ${packageId} because npm returned HTTP 409 Conflict`)
} else {
  process.stdout.write(result.stdout)
  process.stderr.write(result.stderr)
  process.exitCode = result.status ?? 1
}
