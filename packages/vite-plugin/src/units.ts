export type CssValueKind =
  | 'length' | 'length-percentage' | 'number' | 'integer' | 'percentage'
  | 'angle' | 'time' | 'frequency' | 'resolution' | 'flex'
  | 'color' | 'image' | 'url' | 'transform-function' | 'transform-list'

/** An explicitly typed CSS string, for example a dynamic CSS math expression. */
export type CssValue<Kind extends CssValueKind> = string & { readonly __xcssCssValue__: Kind }

const cssNumber = /^[+-]?(?:\d*\.\d+|\d+)(?:[eE][+-]?\d+)?$/

function withUnit<Unit extends string>(unit: Unit) {
  return (value: number | string): `${number}${Unit}` => {
    const numeric = typeof value === 'number' ? String(value) : value.trim()
    if (!cssNumber.test(numeric) || !Number.isFinite(Number(numeric))) {
      throw new TypeError('Expected a finite CSS number')
    }
    return `${numeric}${unit}` as `${number}${Unit}`
  }
}

export const px = withUnit('px')
export const em = withUnit('em')
export const rem = withUnit('rem')
export const vh = withUnit('vh')
export const vw = withUnit('vw')
export const vmin = withUnit('vmin')
export const vmax = withUnit('vmax')
export const dvh = withUnit('dvh')
export const dvw = withUnit('dvw')
export const svh = withUnit('svh')
export const lvh = withUnit('lvh')
export const ch = withUnit('ch')
export const ex = withUnit('ex')
export const cm = withUnit('cm')
export const mm = withUnit('mm')
export const pt = withUnit('pt')
export const pc = withUnit('pc')
export const q = withUnit('q')
export const lh = withUnit('lh')
export const rlh = withUnit('rlh')
export const cqw = withUnit('cqw')
export const cqh = withUnit('cqh')
export const cqi = withUnit('cqi')
export const cqb = withUnit('cqb')
export const rex = withUnit('rex')
export const cap = withUnit('cap')
export const rcap = withUnit('rcap')
export const rch = withUnit('rch')
export const ic = withUnit('ic')
export const ric = withUnit('ric')
export const vi = withUnit('vi')
export const vb = withUnit('vb')
export const svw = withUnit('svw')
export const lvw = withUnit('lvw')
export const cqmin = withUnit('cqmin')
export const cqmax = withUnit('cqmax')
export const deg = withUnit('deg')
export const rad = withUnit('rad')
export const grad = withUnit('grad')
export const turn = withUnit('turn')
export const s = withUnit('s')
export const ms = withUnit('ms')
export const hz = withUnit('hz')
export const khz = withUnit('khz')
export const dpi = withUnit('dpi')
export const dpcm = withUnit('dpcm')
export const dppx = withUnit('dppx')
export const x = withUnit('x')
export const fr = withUnit('fr')
export const inches = withUnit('in')
export const percentage = withUnit('%')
