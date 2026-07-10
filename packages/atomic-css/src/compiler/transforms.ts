export type TransformAxis =
  | 'translate'
  | 'translateX'
  | 'translateY'
  | 'rotate'
  | 'rotateX'
  | 'rotateY'
  | 'rotateZ'
  | 'scale'
  | 'scaleX'
  | 'scaleY'
  | 'skew'
  | 'skewX'
  | 'skewY'

const AXIS_GROUP: Record<TransformAxis, 'translate' | 'rotate' | 'scale' | 'skew'> = {
  translate: 'translate',
  translateX: 'translate',
  translateY: 'translate',
  rotate: 'rotate',
  rotateX: 'rotate',
  rotateY: 'rotate',
  rotateZ: 'rotate',
  scale: 'scale',
  scaleX: 'scale',
  scaleY: 'scale',
  skew: 'skew',
  skewX: 'skew',
  skewY: 'skew',
}

const CSS_VAR: Record<TransformAxis, string> = {
  translate: '--un-translate',
  translateX: '--un-translate-x',
  translateY: '--un-translate-y',
  rotate: '--un-rotate',
  rotateX: '--un-rotate-x',
  rotateY: '--un-rotate-y',
  rotateZ: '--un-rotate-z',
  scale: '--un-scale',
  scaleX: '--un-scale-x',
  scaleY: '--un-scale-y',
  skew: '--un-skew',
  skewX: '--un-skew-x',
  skewY: '--un-skew-y',
}

export type TransformWrite = {
  properties: Record<string, string>
  needsPreflight: true
}

/** Compose transform axes via shared custom properties; base/axis overlap fails. */
export function composeTransforms(
  inputs: Array<{ axis: TransformAxis; value: string }>,
): TransformWrite {
  const groupOwner: Partial<Record<'translate' | 'rotate' | 'scale' | 'skew', 'base' | 'axis'>> =
    {}
  const properties: Record<string, string> = {}

  for (const input of inputs) {
    const group = AXIS_GROUP[input.axis]
    const mode = input.axis === group ? 'base' : 'axis'
    const existing = groupOwner[group]
    if (existing && existing !== mode) {
      throw new Error(
        `transform-overlap: ${group} base and axis forms cannot combine`,
      )
    }
    groupOwner[group] = mode
    properties[CSS_VAR[input.axis]] = input.value
  }

  properties.transform =
    'translate(var(--un-translate-x,0),var(--un-translate-y,0)) rotate(var(--un-rotate,0)) scale(var(--un-scale-x,1),var(--un-scale-y,1)) skew(var(--un-skew-x,0),var(--un-skew-y,0))'

  return { properties, needsPreflight: true }
}

export const TRANSFORM_PREFLIGHT = `:root, :host {
  --un-translate-x: 0;
  --un-translate-y: 0;
  --un-rotate: 0;
  --un-scale-x: 1;
  --un-scale-y: 1;
  --un-skew-x: 0;
  --un-skew-y: 0;
}
`
