import type { CssDeclarations, CssValuePart, EditorRange, EditorPosition } from './index';
export type { CssDeclarations, CssDeclaration, CssValuePart } from './index';
export interface CssValidationOptions { checkCssProperties: boolean; checkCssValues: boolean }
export interface CssWarning { source: string; severity: 2; message: string; range: EditorRange }
export const defaultCssValidation: CssValidationOptions;
export const representatives: Readonly<Record<string, string>>;
export function advance(start: EditorPosition, text: string): EditorPosition;
export function project(parts: CssValuePart[]): { text: string; spans: { start: number; end: number; part: CssValuePart }[] } | null;
export function validateCss(input: CssDeclarations, options?: CssValidationOptions, log?: (message: string) => void): CssWarning[];
