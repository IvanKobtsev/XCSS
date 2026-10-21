import { normalizePath } from 'vite'

export const XCSS_SOURCE_PARAM = 'xcss-source'

const XCSS_EXTENSION = /\.xcss$/i

export interface VirtualXcssId {
  id: string
  sourcePath: string
}

export function isCrossCssId(id: string): boolean {
  return XCSS_EXTENSION.test(splitId(id).path)
}

export function createVirtualId(resolvedId: string): VirtualXcssId | null {
  const { path, query } = splitId(resolvedId)
  if (!XCSS_EXTENSION.test(path)) return null

  const sourcePath = normalizeXcssPath(path)
  const virtualPath = sourcePath.replace(XCSS_EXTENSION, '.module.scss')
  const params = new URLSearchParams(query)
  params.set(XCSS_SOURCE_PARAM, sourcePath)
  return { id: `${virtualPath}?${params}`, sourcePath }
}

export function parseVirtualId(id: string): VirtualXcssId | null {
  const { path, query } = splitId(id)
  const params = new URLSearchParams(query)
  const sourcePath = params.get(XCSS_SOURCE_PARAM)
  if (!sourcePath || !/\.module\.scss$/i.test(path)) return null
  return {
    id,
    sourcePath: normalizeXcssPath(sourcePath),
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
