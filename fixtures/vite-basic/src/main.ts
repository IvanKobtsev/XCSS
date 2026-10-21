import styles from './styles.xcss'
import theme from './theme.xcss'
import inlineCss from './inline.xcss?inline'
import varsA from './vars-a.xcss'
import varsB from './vars-b.xcss'

document.querySelector<HTMLDivElement>('#app')!.className = `${styles.card} ${theme.theme}`
document.querySelector<HTMLDivElement>('#app')!.dataset.inline = inlineCss
document.querySelector<HTMLDivElement>('#app')!.dataset.vars = `${varsA.toneA} ${varsB.toneB}`
