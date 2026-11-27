import type { ClassName } from '@cross-css/vite/types'
import { styles } from './box.xcss'
export { styles }
export function App() {
  const className: ClassName = styles.box
  return <div class="existing" xClass={[className, false, styles.box]} xVars={{ '--width': '24px' }} />
}
