// Adapted from clsx v2.1.1 by Luke Edwards. See CLSX-LICENSE.
function toValue(value: unknown): string {
  if (typeof value === 'string' || typeof value === 'number') return String(value)
  let result = ''
  if (Array.isArray(value)) {
    const length = value.length
    for (let index = 0; index < length; index++) {
      if (!value[index]) continue
      const part = toValue(value[index])
      if (part) result += result ? ' ' + part : part
    }
  } else if (typeof value === 'object' && value !== null) {
    const classes = value as Record<string, unknown>
    for (const key in classes) {
      if (classes[key]) result += result ? ' ' + key : key
    }
  }
  return result
}

export function clsx(...values: unknown[]): string {
  let result = ''
  for (const value of values) {
    if (!value) continue
    const part = toValue(value)
    if (part) result += result ? ' ' + part : part
  }
  return result
}
