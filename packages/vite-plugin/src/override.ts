import type { ClassName } from './types.js'

/** Explicit specificity, validated and lowered by the XCSS compiler. */
export const override: ((className: ClassName, specificity?: number) => ClassName) & {
  readonly __xcssOverrideHelper__: true
} = Object.assign((_className: ClassName, _specificity?: number): ClassName => {
  throw new Error('override() must be compiled by the XCSS Vite plugin')
}, { __xcssOverrideHelper__: true as const })

/** @internal A compiled call evaluates its input once and accepts only its traced origins. */
export function applyOverride(value: ClassName, entries: readonly (readonly [string, string])[]): ClassName {
  const entry = entries.find(([input]) => input === value)
  if (!entry) throw new Error('override() received an unexpected or already-overridden class value')
  return entry[1] as ClassName
}

/** @internal Resolve typed variant leaves before ordinary clsx normalization. */
export function resolveVariants(value: any, entries: readonly (readonly [string, string])[]): any {
  if (typeof value === 'string') return entries.find(([input]) => input === value)?.[1] ?? value
  if (Array.isArray(value)) return value.map(item => resolveVariants(item, entries))
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [resolveVariants(key, entries), item]))
  }
  return value
}

/** @internal Translate only the consumed prop; preserve shared source objects. */
export function resolveVariantProps(value: any, entries: readonly (readonly [string, string])[]): any {
  const props = { ...value }
  if (Object.hasOwn(props, 'xClass')) props.xClass = resolveVariants(props.xClass, entries)
  return props
}
