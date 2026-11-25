import ts from 'typescript'

// Keep the original expressions inside getters so Solid owns their tracking.
export function transformSolid(code: string, id: string) {
  const source = ts.createSourceFile(id, code, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX)
  const edits: { start: number; end: number; text: string }[] = []
  function textFor(node: ts.Node): string {
    const start = node.getStart(source)
    const end = node.end
    let text = code.slice(start, end)
    for (const edit of edits.filter(edit => edit.start >= start && edit.end <= end).sort((a, b) => b.start - a.start)) {
      text = text.slice(0, edit.start - start) + edit.text + text.slice(edit.end - start)
    }
    return text
  }
  function visit(node: ts.Node) {
    ts.forEachChild(node, visit)
    if ((ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) &&
      ts.isIdentifier(node.tagName) && /^[a-z]/.test(node.tagName.text)) {
      const attrs = node.attributes.properties
      if (attrs.some(a => ts.isJsxSpreadAttribute(a) || ts.isJsxAttribute(a) && ['xClass', 'xVars'].includes(a.name.getText(source)))) {
        const fields = attrs.map(a => {
          if (ts.isJsxSpreadAttribute(a)) return `() => (${textFor(a.expression)})`
          const name = a.name.getText(source)
          const value = !a.initializer ? 'true' : ts.isJsxExpression(a.initializer)
            ? a.initializer.expression ? textFor(a.initializer.expression) : 'undefined' : a.initializer.getText(source)
          return `{ get ${JSON.stringify(name)}() { return (${value}); } }`
        })
        for (let i = edits.length - 1; i >= 0; i--) {
          if (edits[i].start >= node.attributes.pos && edits[i].end <= node.attributes.end) edits.splice(i, 1)
        }
        edits.push({ start: node.attributes.pos, end: node.attributes.end,
          text: ` {...__xcssSolidProps([${fields.join(',')}])} ` })
      }
    }
  }
  visit(source)
  if (!edits.length) return
  let output = code
  for (const edit of edits.sort((a, b) => b.start - a.start)) {
    output = output.slice(0, edit.start) + edit.text + output.slice(edit.end)
  }
  return { code: `import { solidProps as __xcssSolidProps } from '@cross-css/vite/solidjs';\n` + output, map: null }
}
