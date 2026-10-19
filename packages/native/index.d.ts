// Generated from the napi-rs exports in xcss-node. Do not edit by hand.
export interface NativeSpan {
  start: number
  end: number
}

export interface NativeDiagnostic {
  message: string
  severity: string
  span: NativeSpan
  line: number
  column: number
  filename?: string
}

export interface NativeTransformResult {
  code: string | null
  diagnostics: NativeDiagnostic[]
}

export function transform(
  source: string,
  syntax: 'css' | 'scss',
  filename?: string | null,
): NativeTransformResult
