export type ClassName = string & { readonly __xcssClassName__: true }

type XClassSlot = string | boolean | null | undefined
export type XClass = readonly [] | readonly [XClassSlot, ...XClassSlot[]]
