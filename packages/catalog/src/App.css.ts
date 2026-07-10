import { atomic } from '@jellypilot/atomic-css'

export const pageStyle = atomic({
  display: 'flex',
  flexDirection: 'column',
  gap: 4,
  p: 6,
  width: 'full',
})

export const sectionStyle = atomic({
  display: 'flex',
  flexDirection: 'column',
  gap: 3,
  p: 0,
  m: 0,
})
