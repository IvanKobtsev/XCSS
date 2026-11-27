import type {} from 'react'
import type { XClass } from '../types.js'
import { clsx } from './clsx.js'

type XVars = Readonly<Record<string, string | number | undefined>>

// Extend the application's existing React JSX types.
declare module 'react' {
  interface HTMLAttributes<T> {
    xVars?: XVars | null | undefined
    xClass?: XClass
  }
  interface SVGAttributes<T> {
    xVars?: XVars | null | undefined
    xClass?: XClass
  }
}

export function normalizeProps(type: unknown, props: unknown): unknown {
  if (typeof type !== 'string' || props === null || typeof props !== 'object'
    || (!Object.hasOwn(props, 'xVars') && !Object.hasOwn(props, 'xClass'))) {
    return props
  }
  const { xVars, xClass, ...rest } = props as {
    xVars?: XVars | null
    xClass?: XClass
    style?: object
    className?: string
  }
  if (Object.hasOwn(props, 'xVars') && xVars != null && Object.keys(xVars).length !== 0) {
    rest.style = { ...rest.style, ...xVars }
  }
  if (Object.hasOwn(props, 'xClass') && xClass != null) {
    const classes = clsx(xClass)
    if (classes) rest.className = clsx(rest.className, classes)
  }
  return rest
}
