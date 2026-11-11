import { jsx as reactJsx, jsxs as reactJsxs } from 'react/jsx-runtime'
import { normalizeProps } from './props.js'

export { Fragment } from 'react/jsx-runtime'
export type { JSX } from './props.js'

export const jsx: typeof reactJsx = (type, props, key) => reactJsx(type, normalizeProps(type, props), key)
export const jsxs: typeof reactJsxs = (type, props, key) => reactJsxs(type, normalizeProps(type, props), key)
