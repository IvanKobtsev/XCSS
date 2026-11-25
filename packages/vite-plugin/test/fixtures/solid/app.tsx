import { styles } from './box.xcss'
export { styles }
export function App() {
  return <div class="existing" xClass={styles.box} xVars={{ '--width': '24px' }} />
}
