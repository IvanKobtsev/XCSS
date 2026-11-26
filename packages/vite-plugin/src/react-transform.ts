import ts from 'typescript'

// Lower only affected host elements. Let TypeScript handle JSX text, entities,
// boolean attributes and spread ordering, but keep every user expression intact
// (including await/yield and TypeScript syntax) for the application's compiler.
export function transformReact(code: string, id: string) {
  const source = ts.createSourceFile(id, code, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX)
  let helper = '__xcssReactElement'
  while (code.includes(helper)) helper += '_'
  let changed = false
  function render(node: ts.Node): string {
    const start = ts.isSourceFile(node) ? 0 : node.getStart(source)
    const edits: { start: number; end: number; text: string }[] = []
    function children(child: ts.Node) {
      const text = render(child)
      if (text !== code.slice(child.getStart(source), child.end)) {
        const jsxChild = (ts.isJsxElement(node) || ts.isJsxFragment(node) || ts.isJsxAttribute(node)) &&
          (ts.isJsxElement(child) || ts.isJsxSelfClosingElement(child) || ts.isJsxFragment(child))
        edits.push({ start: child.getStart(source), end: child.end, text: jsxChild ? '{' + text + '}' : text })
      }
    }
    const opening = ts.isJsxElement(node) ? node.openingElement : ts.isJsxSelfClosingElement(node) ? node : undefined
    if (opening && ts.isIdentifier(opening.tagName) && /^[a-z]/.test(opening.tagName.text) &&
      opening.attributes.properties.some(a => ts.isJsxSpreadAttribute(a) ||
        ts.isJsxAttribute(a) && ['xClass', 'xVars', '__xcssClassSlots'].includes(a.name.getText(source)))) {
      changed = true
      const values: string[] = []
      let marker = '__xcssExpression'
      while (code.includes(marker)) marker += '_'
      function capture(value: ts.Node, jsx = false) {
        const name = marker + values.length
        values.push(render(value))
        edits.push({ start: value.getStart(source), end: value.end, text: jsx ? '{' + name + '}' : name })
      }
      // Capture expressions and child elements before lowering, so no user code
      // is moved into a new scope or evaluated more than once.
      function visit(child: ts.Node) {
        if (ts.isJsxExpression(child)) {
          if (child.expression) capture(child.expression)
        } else if (ts.isJsxSpreadAttribute(child)) capture(child.expression)
        else if (ts.isJsxElement(child) || ts.isJsxSelfClosingElement(child) || ts.isJsxFragment(child)) capture(child, true)
        else ts.forEachChild(child, visit)
      }
      ts.forEachChild(node, visit)
      let jsx = code.slice(start, node.end)
      for (const edit of edits.sort((a, b) => b.start - a.start)) {
        jsx = jsx.slice(0, edit.start - start) + edit.text + jsx.slice(edit.end - start)
      }
      const lowered = ts.transpileModule('const result = ' + jsx, {
        compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
      }).outputText
      const parsed = ts.createSourceFile('lowered.js', lowered, ts.ScriptTarget.Latest, true)
      const factories = new Map<string, string>()
      for (const statement of parsed.statements) {
        if (ts.isImportDeclaration(statement)) {
          const bindings = statement.importClause?.namedBindings
          if (bindings && ts.isNamedImports(bindings)) for (const binding of bindings.elements) {
            factories.set(binding.name.text, binding.propertyName?.text === 'createElement' ? helper + 'Fallback' : helper)
          }
        }
      }
      const statement = parsed.statements.find(ts.isVariableStatement)!
      const expression = statement.declarationList.declarations[0].initializer!
      const replacements: { start: number; end: number; text: string }[] = []
      function restore(node: ts.Node) {
        if (ts.isIdentifier(node)) {
          const factory = ts.isCallExpression(node.parent) && node.parent.expression === node
            ? factories.get(node.text) : undefined
          const index = node.text.startsWith(marker) ? Number(node.text.slice(marker.length)) : NaN
          const text = factory ?? (Number.isInteger(index) ? '(' + values[index] + ')' : undefined)
          if (text !== undefined) replacements.push({ start: node.getStart(parsed), end: node.end, text })
        }
        ts.forEachChild(node, restore)
      }
      restore(expression)
      let call = expression.getText(parsed)
      const offset = expression.getStart(parsed)
      for (const edit of replacements.sort((a, b) => b.start - a.start)) {
        call = call.slice(0, edit.start - offset) + edit.text + call.slice(edit.end - offset)
      }
      return call
    }
    ts.forEachChild(node, children)
    let text = code.slice(start, node.end)
    for (const edit of edits.sort((a, b) => b.start - a.start)) {
      text = text.slice(0, edit.start - start) + edit.text + text.slice(edit.end - start)
    }
    return text
  }
  const output = render(source)
  if (!changed) return
  let importOffset = output.startsWith('#!') ? output.indexOf('\n') + 1 : 0
  const parsed = ts.createSourceFile(id, output, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX)
  for (const statement of parsed.statements) {
    if (!ts.isExpressionStatement(statement) || !ts.isStringLiteral(statement.expression)) break
    importOffset = statement.end
  }
  return { code: output.slice(0, importOffset) + `\nimport { jsx as ${helper}, createElement as ${helper}Fallback } from '@cross-css/vite/react';\n` + output.slice(importOffset), map: null }
}
