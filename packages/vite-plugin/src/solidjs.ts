import type { JSX } from '@solidjs/web'
import { normalizeProps } from './react/props.js'
import type { XClass } from './types.js'

declare module '@solidjs/web/jsx-runtime' {
  namespace JSX {
    interface HTMLAttributes<T> {
      xClass?: XClass
      xVars?: Readonly<Record<string, string | number | undefined>> | null
    }
    interface SVGAttributes<T> {
      xClass?: XClass
      xVars?: Readonly<Record<string, string | number | undefined>> | null
    }
  }
}

export function solidProps(sources: readonly (Record<string, any> | (() => Record<string, any>))[]): Record<string, any> {
  const objects = () => sources.map(source => typeof source === 'function' ? source() : source)
  const props = new Proxy({} as Record<string, any>, {
    get: (_, key) => { for (const object of objects().reverse()) if (object != null && key in object) return object[key as string] },
    ownKeys: () => [...new Set(objects().flatMap(object => object == null ? [] : Reflect.ownKeys(object)))],
    getOwnPropertyDescriptor: () => ({ enumerable: true, configurable: true }),
    has: (_, key) => objects().some(object => object != null && key in object),
  })
  const rest = new Proxy(props, {
    ownKeys: target => Reflect.ownKeys(target).filter(key => !['xClass', 'xVars', '__xcssClassSlots', 'class', 'style'].includes(String(key))),
    getOwnPropertyDescriptor: (target, key) => Object.getOwnPropertyDescriptor(target, key),
  })
  const bindings = {
    get class() {
      const resolved = normalizeProps('div', { xClass: props.xClass, __xcssClassSlots: props.__xcssClassSlots }) as { className?: string }
      return [props.class, resolved.className] as JSX.HTMLAttributes<HTMLElement>['class']
    },
    get style() {
      const style = props.style
      if (props.xVars == null) return style
      const base = typeof style === 'string'
        ? Object.fromEntries(style.split(';').filter(Boolean).map(item => { const i = item.indexOf(':'); return [item.slice(0, i).trim(), item.slice(i + 1).trim()] }))
        : style
      return { ...base, ...props.xVars }
    },
  }
  return new Proxy(rest, {
    get: (target, key) => key === 'class' || key === 'style' ? bindings[key] : Reflect.get(target, key),
    ownKeys: target => [...new Set([...Reflect.ownKeys(target), 'class', 'style'])],
    getOwnPropertyDescriptor: (target, key) => key === 'class' || key === 'style'
      ? { enumerable: true, configurable: true } : Object.getOwnPropertyDescriptor(target, key),
    has: (target, key) => key === 'class' || key === 'style' || Reflect.has(target, key),
  })
}
