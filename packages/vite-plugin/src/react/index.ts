import { createElement as reactCreateElement } from 'react'
import { withXVars } from './props.js'

export type { JSX } from './props.js'

// The automatic JSX transform uses createElement when a key follows a spread.
export const createElement = ((...args: Parameters<typeof reactCreateElement>) => {
  const [type, props, ...children] = args
  return reactCreateElement(type, withXVars(type, props) as typeof props, ...children)
}) as typeof reactCreateElement
