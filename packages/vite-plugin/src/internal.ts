import { normalizePath } from 'vite'
export const XCSS_SOURCE_PARAM = 'xcss-source'
export interface VirtualXcssId { id: string; sourcePath: string; kind: 'module' | 'css' | 'bundle'; mode: 'module' | 'inline' | 'url' }
export function isCrossCssId(id: string): boolean { return /\.xcss$/i.test(id.split('?', 1)[0]) }
export function createVirtualId(resolvedId: string): VirtualXcssId | null {
  const [path, query = ''] = resolvedId.split('?')
  if (!isCrossCssId(path)) return null
  const params = new URLSearchParams(query)
  if (params.has('inline') && params.has('url')) throw new Error('XCSS cannot combine ?inline and ?url')
  for (const key of params.keys()) if (!['inline', 'url', 't', 'import'].includes(key)) throw new Error('Unsupported XCSS query: ' + key)
  const sourcePath = normalizePath(path.replaceAll('\\', '/'))
  params.set(XCSS_SOURCE_PARAM, sourcePath)
  params.set('xcss-kind', 'module')
  return { id: sourcePath + '.js?' + params, sourcePath, kind: 'module', mode: params.has('inline') ? 'inline' : params.has('url') ? 'url' : 'module' }
}
export function createCssId(sourcePath: string, bundle = false, mode = ''): string {
  const params = new URLSearchParams({ [XCSS_SOURCE_PARAM]: sourcePath, 'xcss-kind': bundle ? 'bundle' : 'css' })
  return sourcePath + '.css?' + params + (mode ? '&' + mode : '')
}
export function parseVirtualId(id: string): VirtualXcssId | null {
  const query = id.indexOf('?')
  if (query === -1) return null
  const params = new URLSearchParams(id.slice(query + 1))
  const sourcePath = params.get(XCSS_SOURCE_PARAM)
  const kind = params.get('xcss-kind')
  if (!sourcePath || !['module', 'css', 'bundle'].includes(kind ?? '')) return null
  return { id, sourcePath: normalizePath(sourcePath.replaceAll('\\', '/')), kind: kind as VirtualXcssId['kind'], mode: params.has('inline') ? 'inline' : params.has('url') ? 'url' : 'module' }
}
