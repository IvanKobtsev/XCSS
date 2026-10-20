import styles from './styles.cross.css'
import theme from './theme.cross.scss'
import inlineCss from './inline.cross.css?inline'
import varsA from './vars-a.cross.css'
import varsB from './vars-b.cross.css'

document.querySelector<HTMLDivElement>('#app')!.className = `${styles.card} ${theme.theme}`
document.querySelector<HTMLDivElement>('#app')!.dataset.inline = inlineCss
document.querySelector<HTMLDivElement>('#app')!.dataset.vars = `${varsA.toneA} ${varsB.toneB}`
