/// <reference path="../../../client.d.ts" />
import { override as boost } from '@cross-css/vite'
import { styles } from './box.xcss'

export const boosted = boost(styles.override, 2)
