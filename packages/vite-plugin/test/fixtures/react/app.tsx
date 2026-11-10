import { createRef } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { px } from '@cross-css/vite/units'
import styles, { cssVars } from './box.xcss'
import { cssVars as emptyVars } from './empty.xcss'

export { cssVars, emptyVars }
export const ref = createRef<HTMLDivElement>()
const style = { height: '200px', [cssVars.providedWidth]: '1px' }
const provided = { xVars: { [cssVars.providedWidth]: px(24) } }

export const element = <div {...provided} key="box" ref={ref} className={styles.box} style={style}><b>A</b><i>B</i></div>
export const withoutStyle = <div xVars={provided.xVars}><b>C</b><i>D</i></div>
export const empty = <div style={style} xVars={{}} />
export const unchanged = <div style={style} />

function Component({ xVars }: { xVars: typeof provided.xVars }) {
  return <span>{xVars[cssVars.providedWidth]}</span>
}

export const html = renderToStaticMarkup(<>{element}{withoutStyle}<Component {...provided} /></>)
