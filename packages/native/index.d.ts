// Generated from napi-rs binding metadata. Do not edit by hand.
export interface EditorPosition {
line: number
character: number
}

export interface EditorRange {
start: EditorPosition
end: EditorPosition
}

export interface EditorLocation {
uri: string
range: EditorRange
}

export interface EditorDiagnostic {
code: string
message: string
severity: string
location: EditorLocation
related: Array<EditorLocation>
}

export interface SemanticToken {
line: number
character: number
length: number
tokenType: number
declaration: boolean
}

export interface EditorDocument {
revision: number
diagnostics: Array<EditorDiagnostic>
tokens: Array<SemanticToken>
}

export interface CssValuePart {
text?: string
valueType?: string
range: EditorRange
}

export interface CssDeclaration {
property: string
propertyRange: EditorRange
valueRange: EditorRange
context: string
checkable: boolean
parts: Array<CssValuePart>
}

export interface CssDeclarations {
revision: number
declarations: Array<CssDeclaration>
}

export interface Specificity {
ids: number
classes: number
types: number
}

export interface SelectorPart {
kind: string
range: EditorRange
}

export interface Selector {
range: EditorRange
parts: Array<SelectorPart>
specificity?: Specificity
reason?: string
}

export interface Selectors {
revision: number
selectors: Array<Selector>
}

export interface EditorHover {
range: EditorRange
text: string
}

export interface LanguageServiceCapabilities {
apiVersion: number
tokenTypes: Array<string>
tokenModifiers: Array<string>
}

export declare function languageServiceCapabilities(): LanguageServiceCapabilities

export interface NativeSpan {
start: number
end: number
}

export interface NativeLocation {
moduleId: string
span: NativeSpan
}

export interface NativeDiagnostic {
code: string
message: string
severity: string
moduleId: string
span: NativeSpan
line: number
column: number
related: Array<NativeLocation>
}

export interface NativeArtifact {
moduleId: string
css: string
sourceMap: string
classes: Record<string, string>
customProperties: Record<string, string>
stylesheetDependencies: Array<string>
nativeImports: Array<string>
sourceDependencies: Array<string>
}

export interface NativeCompilation {
success: boolean
artifacts: Array<NativeArtifact>
diagnostics: Array<NativeDiagnostic>
}

export interface NativeInspection {
imports: Array<string>
}

export declare function inspect(source: string): NativeInspection

export declare class LanguageService {

 selectors(uri: string): Selectors
 cssDeclarations(uri: string): CssDeclarations
 constructor()
 setFile(uri: string, revision: number, source: string): void
 removeFile(uri: string, revision: number): void
 setResolution(importer: string, specifier: string, target?: string | undefined | null): void
 document(uri: string): EditorDocument
 definition(uri: string, position: EditorPosition): Array<EditorLocation>
 references(uri: string, position: EditorPosition, includeDeclaration: boolean): Array<EditorLocation>
 hover(uri: string, position: EditorPosition): EditorHover | null
}

export declare class Compiler {

 constructor()
 setFile(moduleId: string, revision: number, source: string): void
 removeFile(moduleId: string, revision: number): void
 setResolution(importer: string, specifier: string, target?: string | undefined | null): void
 compile(entry: string, minify?: boolean | undefined | null, bundle?: boolean | undefined | null): NativeCompilation
}
