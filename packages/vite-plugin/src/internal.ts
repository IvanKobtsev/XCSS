import { normalizePath } from 'vite'

export const XCSS_SOURCE_PARAM = 'xcss-source'

const CROSS_EXTENSION = /\.cross\.(css|scss)$/i

export interface VirtualXcssId {
  id: string
  sourcePath: string
  syntax: 'css' | 'scss'
}

export function isCrossCssId(id: string): boolean {
  return CROSS_EXTENSION.test(splitId(id).path)
}

export function createVirtualId(resolvedId: string): VirtualXcssId | null {
  const { path, query } = splitId(resolvedId)
  const match = CROSS_EXTENSION.exec(path)
  if (!match) return null

  const sourcePath = normalizeXcssPath(path)
  const syntax = match[1].toLowerCase() as 'css' | 'scss'
  const virtualPath = sourcePath.replace(CROSS_EXTENSION, `.module.${syntax}`)
  const params = new URLSearchParams(query)
  params.set(XCSS_SOURCE_PARAM, sourcePath)
  return { id: `${virtualPath}?${params}`, sourcePath, syntax }
}

export function parseVirtualId(id: string): VirtualXcssId | null {
  const { path, query } = splitId(id)
  const params = new URLSearchParams(query)
  const sourcePath = params.get(XCSS_SOURCE_PARAM)
  const match = /\.module\.(css|scss)$/i.exec(path)
  if (!sourcePath || !match) return null
  return {
    id,
    sourcePath: normalizeXcssPath(sourcePath),
    syntax: match[1].toLowerCase() as 'css' | 'scss',
  }
}

function normalizeXcssPath(path: string): string {
  return normalizePath(path.replaceAll('\\', '/'))
}

function splitId(id: string): { path: string; query: string } {
  const queryStart = id.indexOf('?')
  return queryStart === -1
    ? { path: id, query: '' }
    : { path: id.slice(0, queryStart), query: id.slice(queryStart + 1) }
}
