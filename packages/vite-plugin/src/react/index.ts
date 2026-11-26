import { createElement as reactCreateElement } from 'react'
import { normalizeProps } from './props.js'
// Keep the prop augmentation reachable from the published declaration entry.
import './props.js'

// Match automatic JSX key precedence and coercion, while using React's public
// element API. Keep children in props so array children retain their JSX shape.
export function jsx(type: string, props: Record<string, unknown>, key?: unknown) {
  const normalized = normalizeProps(type, props) as Record<string, unknown>
  const explicitKey = key === undefined ? undefined : '' + key
  return reactCreateElement(type, explicitKey !== undefined && normalized.key === undefined
    ? { ...normalized, key: explicitKey } : normalized)
}

// The automatic JSX transform uses createElement when a key follows a spread.
export const createElement = ((...args: Parameters<typeof reactCreateElement>) => {
  const [type, props, ...children] = args
  return reactCreateElement(type, normalizeProps(type, props) as typeof props, ...children)
}) as typeof reactCreateElement
