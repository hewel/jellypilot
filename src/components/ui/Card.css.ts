import { styleVariants } from '@vanilla-extract/css';

/**
 * Non-atomic card surface treatment: multi-stop gradients and compound shadows
 * that Tailwind utilities cannot express cleanly. Atomic border/padding/radius
 * classes live on the Card component itself.
 */
export const cardSurface = styleVariants({
  elevated: {
    backgroundImage:
      'linear-gradient(135deg, rgba(28, 32, 48, 0.45) 0%, rgba(17, 19, 28, 0.65) 100%)',
    boxShadow: '0 25px 50px -12px rgba(0, 0, 0, 0.6), 0 0 40px -10px rgba(79, 70, 229, 0.12)',
    selectors: {
      '&:hover': {
        boxShadow: '0 30px 60px -12px rgba(0, 0, 0, 0.75), 0 0 50px -5px rgba(79, 70, 229, 0.22)',
      },
    },
  },
  filled: {
    backgroundImage:
      'linear-gradient(135deg, rgba(21, 24, 35, 0.5) 0%, rgba(11, 13, 20, 0.7) 100%)',
  },
  outlined: {},
});
