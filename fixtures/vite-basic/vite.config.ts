import { defineConfig } from 'vite'
import xcss from '@cross-css/vite-plugin'
import { fileURLToPath } from 'node:url'
export default defineConfig({ plugins:[xcss()], resolve:{alias:{'@':fileURLToPath(new URL('./src',import.meta.url))}} })
