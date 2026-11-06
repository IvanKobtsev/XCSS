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

export interface CssFunction {
name: string
range: EditorRange
}

export interface CssValue {
property?: string
text: string
range: EditorRange
parts: Array<CssValuePart>
checkable: boolean
functions: Array<CssFunction>
}

export interface CssValues {
revision: number
values: Array<CssValue>
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
utilities?: NativeUtilities
}

export interface NativeUtilities {
css: string
sourceMap: string
sourceDependencies: Array<string>
rules: Array<NativeUtilityRule>
}

export interface NativeUtilityRule {
key: string
css: string
sourceMap: string
}

export interface NativeInspection {
imports: Array<string>
}

export declare function inspect(source: string): NativeInspection

export interface SourceFormatOptions {
tabWidth?: number
useTabs?: boolean
printWidth?: number
endOfLine?: string
}

export interface FormatDiagnostic {
code: string
message: string
/** UTF-16 positions in the original source, not Rust byte offsets. */
range: EditorRange
}

export interface SourceFormatResult {
text?: string
diagnostics: Array<FormatDiagnostic>
}

export declare function formatSource(source: string, options?: SourceFormatOptions | undefined | null): SourceFormatResult

export declare class LanguageService {

 cssValues(uri: string): CssValues
 selectors(uri: string): Selectors
 cssDeclarations(uri: string): CssDeclarations
 constructor()
 setTypescriptProject(config?: string | undefined | null): void
 setTypescriptFile(path: string, revision: number, source: string): void
 removeTypescriptFile(path: string, revision: number): void
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
 compileUtilities(entries: Array<string>): NativeCompilation
 compile(entry: string, minify?: boolean | undefined | null, bundle?: boolean | undefined | null): NativeCompilation
}
