import type { JSX as ReactJSX } from 'react'

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
    }
  }
}

export function withXVars(type: unknown, props: unknown): unknown {
  if (typeof type !== 'string' || props === null || typeof props !== 'object' || !Object.hasOwn(props, 'xVars')) {
    return props
  }
  const { xVars, ...rest } = props as { xVars?: XVars | null; style?: object }
  if (xVars != null && Object.keys(xVars).length !== 0) {
    rest.style = { ...rest.style, ...xVars }
  }
  return rest
}
