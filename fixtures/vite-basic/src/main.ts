import styles from './styles.xcss'
import theme from './theme.xcss'
import inlineCss from './inline.xcss?inline'
import varsA from './vars-a.xcss'
import varsB from './vars-b.xcss'

document.querySelector<HTMLDivElement>('#app')!.className = `${styles.card} ${theme.theme}`
document.querySelector<HTMLDivElement>('#app')!.dataset.inline = inlineCss
document.querySelector<HTMLDivElement>('#app')!.dataset.vars = `${varsA.toneA} ${varsB.toneB}`

import stylesheetUrl from './inline.xcss?url'
document.querySelector<HTMLDivElement>('#app')!.dataset.stylesheet = stylesheetUrl

import browser from './browser.xcss'
const parent=document.createElement('div'); parent.id='parent';parent.className=browser.parent
const child=document.createElement('div');child.id='child';child.className=browser.child;child.textContent='child';parent.append(child);document.body.append(parent)
const composed=document.createElement('div');composed.id='composed';composed.className=browser.composed;document.body.append(composed)
const animated=document.createElement('div');animated.id='animated';animated.className=browser.animated;document.body.append(animated)
