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
  partial: string | null
  diagnostics: NativeDiagnostic[]
}

export interface NativeProjectFile {
  moduleId: string
  filename: string
  source: string
}

export interface NativeImportResolution {
  importer: string
  specifier: string
  target: string
}

export interface NativeInspection {
  imports: string[]
}

export function inspect(source: string): NativeInspection

export function transform(
  files: NativeProjectFile[],
  resolutions: NativeImportResolution[],
  targetModuleId: string,
  partialPath: string,
): NativeTransformResult
