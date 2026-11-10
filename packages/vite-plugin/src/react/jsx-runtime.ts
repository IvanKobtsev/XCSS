import { jsx as reactJsx, jsxs as reactJsxs } from 'react/jsx-runtime'
import { withXVars } from './props.js'

export { Fragment } from 'react/jsx-runtime'
export type { JSX } from './props.js'

export const jsx: typeof reactJsx = (type, props, key) => reactJsx(type, withXVars(type, props), key)
export const jsxs: typeof reactJsxs = (type, props, key) => reactJsxs(type, withXVars(type, props), key)
