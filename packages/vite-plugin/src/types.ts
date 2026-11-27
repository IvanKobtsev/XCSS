export type ClassName = string & { readonly __xcssClassName__: true }

export type XClass = string | number | boolean | null | undefined | readonly XClass[] | Readonly<Record<string, any>>
