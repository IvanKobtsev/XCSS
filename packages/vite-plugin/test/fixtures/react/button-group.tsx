import type { ClassName } from '@cross-css/vite/react'
import { styles } from './buttons.xcss'
import type { Variants } from './button-variants.xcss'

export function ButtonGroup(props: { className: ClassName; variant: Variants<'Button'> }) {
  const { className, variant } = props
  return <>
    <button id="first" xClass={[styles.firstButton, variant, className]} />
    <button id="second" xClass={[styles.secondButton, className, variant]}>
      <span className="nested-child">Nested selector</span>
    </button>
  </>
}
