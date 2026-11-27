import { override } from '@cross-css/vite'
import type { ClassName } from '@cross-css/vite/types'
import { styles } from './box.xcss'
export { styles }
export function App() {
  const className: ClassName = styles.box
  return <div class="existing" xClass={[className, false, override(styles.box, 2)]} xVars={{ '--width': '24px' }} />
}
