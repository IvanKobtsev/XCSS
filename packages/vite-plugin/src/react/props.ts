import type { JSX as ReactJSX } from 'react'
import type { XClass } from './class-name.js'
import { clsx } from './clsx.js'

type XVars = Readonly<Record<string, string | number | undefined>>

export namespace JSX {
  export type ElementType = ReactJSX.ElementType
  export interface Element extends ReactJSX.Element {}
  export interface ElementClass extends ReactJSX.ElementClass {}
  export interface ElementAttributesProperty extends ReactJSX.ElementAttributesProperty {}
  export interface ElementChildrenAttribute extends ReactJSX.ElementChildrenAttribute {}
  export type LibraryManagedAttributes<C, P> = ReactJSX.LibraryManagedAttributes<C, P>
  export interface IntrinsicAttributes extends ReactJSX.IntrinsicAttributes {}
  export interface IntrinsicClassAttributes<T> extends ReactJSX.IntrinsicClassAttributes<T> {}
  export type IntrinsicElements = {
    [Tag in keyof ReactJSX.IntrinsicElements]: ReactJSX.IntrinsicElements[Tag] & {
      xVars?: XVars | null | undefined
      xClass?: XClass
    }
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
    if (!slots || !Array.isArray(xClass) || xClass.length !== slots.length) {
      throw new Error('xClass requires a fixed tuple compiled by the XCSS Vite plugin')
    }
    const classes = clsx(xClass.map((value, index) => {
      if (value == null || typeof value === 'boolean') return ''
      const entry = slots[index].find(([input]) => input === value)
      if (!entry) throw new Error('xClass received an unrecognized class in slot ' + index)
      return entry[1]
    }))
    if (classes) rest.className = clsx(rest.className, classes)
  }
  return rest
}
