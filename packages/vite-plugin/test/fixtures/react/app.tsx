import { createRef } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { px } from '@cross-css/vite/units'
import { styles, cssVars } from './box.xcss'
import { cssVars as emptyVars } from './empty.xcss'
import { ButtonGroup } from './button-group.js'

export { cssVars, emptyVars, styles }
export const ref = createRef<HTMLDivElement>()
const style = { height: '200px', [cssVars.providedWidth]: '1px' }
const provided = { xVars: { [cssVars.providedWidth]: px(24) }, xClass: [styles.box, false, styles.box] as const }

export const element = <div {...provided} key="box" ref={ref} className="global" style={style}><b>A</b><i>B</i></div>
export const withoutStyle = <div xVars={provided.xVars} xClass={styles.box}><b>C</b><i>D</i></div>
export const empty = <div style={style} xVars={{}} xClass={[false, null, undefined]} />
export const unchanged = <div style={style} className=" kept " xClass={[]} />

function Component({ xVars }: { xVars: typeof provided.xVars }) {
  return <span>{xVars[cssVars.providedWidth]}</span>
}

function ForwardedButtons(props: Parameters<typeof ButtonGroup>[0]) {
  return <ButtonGroup {...props} />
}

export const html = renderToStaticMarkup(<>{element}{withoutStyle}<Component {...provided} />
  <ForwardedButtons className={styles.override} variant="secondary" />
</>)
