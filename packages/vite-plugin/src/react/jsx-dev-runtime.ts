import { jsxDEV as reactJsxDEV } from 'react/jsx-dev-runtime'
import { normalizeProps } from './props.js'

export { Fragment } from 'react/jsx-dev-runtime'
export type { JSX } from './props.js'

export const jsxDEV: typeof reactJsxDEV = (type, props, key, ...debug) =>
  reactJsxDEV(type, normalizeProps(type, props), key, ...debug)
