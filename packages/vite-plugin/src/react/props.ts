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
    || (!Object.hasOwn(props, 'xVars') && !Object.hasOwn(props, 'xClass') && !Object.hasOwn(props, '__xcssClassSlots'))) {
    return props
  }
  const { xVars, xClass, __xcssClassSlots: slots, ...rest } = props as {
    xVars?: XVars | null
    xClass?: XClass
    __xcssClassSlots?: readonly (readonly (readonly [string, string])[])[]
    style?: object
    className?: string
  }
  if (Object.hasOwn(props, 'xVars') && xVars != null && Object.keys(xVars).length !== 0) {
    rest.style = { ...rest.style, ...xVars }
  }
  if (Object.hasOwn(props, 'xClass') && xClass != null) {
    const values = typeof xClass === 'string' || typeof xClass === 'boolean' ? [xClass] : xClass
    if (!slots || values.length !== slots.length) {
      throw new Error('xClass requires a class or fixed tuple compiled by the XCSS Vite plugin')
    }
    const classes = clsx(values.map((value, index) => {
      if (value == null || typeof value === 'boolean') return ''
      const entry = slots[index].find(([input]) => input === value)
      if (!entry) throw new Error('xClass received an unrecognized class in slot ' + index)
      return entry[1]
    }))
    if (classes) rest.className = clsx(rest.className, classes)
  }
  return rest
}
