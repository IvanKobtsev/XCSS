import styles from './styles.cross.css'
import theme from './theme.cross.scss'
import inlineCss from './inline.cross.css?inline'

document.querySelector<HTMLDivElement>('#app')!.className = `${styles.card} ${theme.theme}`
document.querySelector<HTMLDivElement>('#app')!.dataset.inline = inlineCss
