import { tabsAnatomy } from '@ark-ui/solid/anatomy'
import { defineConfig, defineSlotRecipe } from '@pandacss/dev'

const sidebarStyles = defineSlotRecipe({
  className: 'sidebar',
  slots: tabsAnatomy.keys(),
  base: {
    root: {
      height: '100vh',
      width: '100vw',
      display: 'flex',
      backgroundColor: 'background',
      overflow: 'hidden'
    },
    list: {
      width: '80px',
      flexShrink: 0,
      backgroundColor: 'surfaceContainerLow',
      borderRightWidth: '1px',
      borderRightColor: 'outlineVariant',
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      paddingY: '24px',
      gap: '16px'
    },
    trigger: {
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      justifyContent: 'center',
      gap: '4px',
      width: '64px',
      height: '64px',
      borderRadius: '16px',
      cursor: 'pointer',
      transition: 'all 0.2s',
      color: 'onSurfaceVariant',
      backgroundColor: 'transparent',
      border: 'none',
      _hover: {
        backgroundColor: 'surfaceContainerHigh',
        color: 'onSurface'
      },
      _selected: {
        backgroundColor: 'secondaryContainer',
        color: 'onSecondaryContainer'
      }
    },
  }
})

export default defineConfig({
  // Whether to use css reset
  preflight: true,

  // Where to look for your css declarations
  include: ['./src/**/*.{js,jsx,ts,tsx}'],

  // Files to exclude
  exclude: [],

  // Solid.js framework
  jsxFramework: 'solid',

  // The output directory for your css system
  outdir: 'styled-system',

  // Theme customization - Material Design 3 Dark Theme
  theme: {
    extend: {
      // M3 Color Tokens (Jellyfin seed color #00A4DC)
      // Keyframes for animations
      keyframes: {
        spin: {
          from: { transform: 'rotate(0deg)' },
          to: { transform: 'rotate(360deg)' }
        },
        pulse: {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.5' }
        },
        fadeIn: {
          from: { opacity: '0' },
          to: { opacity: '1' }
        },
        slideInFromTop: {
          from: { transform: 'translateY(-4px)', opacity: '0' },
          to: { transform: 'translateY(0)', opacity: '1' }
        },
        slideInFromBottom: {
          from: { transform: 'translateY(8px)', opacity: '0' },
          to: { transform: 'translateY(0)', opacity: '1' }
        },
        slideInFromRight: {
          from: { transform: 'translateX(100%)', opacity: '0' },
          to: { transform: 'translateX(0)', opacity: '1' }
        }
      },

      tokens: {
        colors: {
          // Primary colors
          primary: { value: '#8fcef3' },
          onPrimary: { value: '#003549' },
          primaryContainer: { value: '#004c69' },
          onPrimaryContainer: { value: '#c3e7ff' },

          // Secondary colors
          secondary: { value: '#b5c9d7' },
          onSecondary: { value: '#20333e' },
          secondaryContainer: { value: '#364955' },
          onSecondaryContainer: { value: '#d1e5f4' },

          // Tertiary colors
          tertiary: { value: '#cac1ea' },
          onTertiary: { value: '#322c4c' },
          tertiaryContainer: { value: '#484264' },
          onTertiaryContainer: { value: '#e6deff' },

          // Error colors
          error: { value: '#ffb4ab' },
          onError: { value: '#690005' },
          errorContainer: { value: '#93000a' },
          onErrorContainer: { value: '#ffdad6' },

          // Surface colors
          surface: { value: '#0f1417' },
          onSurface: { value: '#dfe3e7' },
          surfaceVariant: { value: '#40484c' },
          onSurfaceVariant: { value: '#c0c8cc' },

          // Surface Containers (M3 Elevation replacement)
          surfaceContainerLowest: { value: '#0a0f11' },
          surfaceContainerLow: { value: '#171c1f' },
          surfaceContainer: { value: '#1b2023' },
          surfaceContainerHigh: { value: '#252b2e' },
          surfaceContainerHighest: { value: '#303639' },

          // Outline
          outline: { value: '#8a9296' },
          outlineVariant: { value: '#40484c' },

          // Background
          background: { value: '#0f1417' },
          onBackground: { value: '#dfe3e7' },

          // Shadow & Scrim
          shadow: { value: '#000000' },
          scrim: { value: '#000000' }
        },
        fonts: {
          sans: { value: '"Roboto", ui-sans-serif, system-ui, sans-serif' },
          mono: {
            value:
              'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace'
          }
        },
        letterSpacings: {
          tighter: { value: '-0.05em' },
          tight: { value: '-0.025em' },
          normal: { value: '0em' },
          wide: { value: '0.025em' },
          wider: { value: '0.05em' },
          widest: { value: '0.1em' }
        },
        lineHeights: {
          relaxed: { value: '1.625' }
        },
        shadows: {
          sm: { value: '0 1px 2px 0 rgb(0 0 0 / 0.05)' },
          md: { value: '0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)' },
          lg: { value: '0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)' },
          xl: { value: '0 20px 25px -5px rgb(0 0 0 / 0.1), 0 8px 10px -6px rgb(0 0 0 / 0.1)' }
        }
      },

      // M3 Text Styles
      textStyles: {
        // Display styles (Large, impressive headers)
        displayLarge: {
          value: {
            fontSize: '57px',
            lineHeight: '64px',
            fontWeight: '400',
            letterSpacing: '-0.25px'
          }
        },
        displayMedium: {
          value: {
            fontSize: '45px',
            lineHeight: '52px',
            fontWeight: '400',
            letterSpacing: '0px'
          }
        },
        displaySmall: {
          value: {
            fontSize: '36px',
            lineHeight: '44px',
            fontWeight: '400',
            letterSpacing: '0px'
          }
        },

        // Headline styles (Section headers)
        headlineLarge: {
          value: {
            fontSize: '32px',
            lineHeight: '40px',
            fontWeight: '400',
            letterSpacing: '0px'
          }
        },
        headlineMedium: {
          value: {
            fontSize: '28px',
            lineHeight: '36px',
            fontWeight: '400',
            letterSpacing: '0px'
          }
        },
        headlineSmall: {
          value: {
            fontSize: '24px',
            lineHeight: '32px',
            fontWeight: '400',
            letterSpacing: '0px'
          }
        },

        // Title styles (Card titles, Dialog titles)
        titleLarge: {
          value: {
            fontSize: '22px',
            lineHeight: '28px',
            fontWeight: '400',
            letterSpacing: '0px'
          }
        },
        titleMedium: {
          value: {
            fontSize: '16px',
            lineHeight: '24px',
            fontWeight: '500',
            letterSpacing: '0.15px'
          }
        },
        titleSmall: {
          value: {
            fontSize: '14px',
            lineHeight: '20px',
            fontWeight: '500',
            letterSpacing: '0.1px'
          }
        },

        // Body styles (Reading text)
        bodyLarge: {
          value: {
            fontSize: '16px',
            lineHeight: '24px',
            fontWeight: '400',
            letterSpacing: '0.5px'
          }
        },
        bodyMedium: {
          value: {
            fontSize: '14px',
            lineHeight: '20px',
            fontWeight: '400',
            letterSpacing: '0.25px'
          }
        },
        bodySmall: {
          value: {
            fontSize: '12px',
            lineHeight: '16px',
            fontWeight: '400',
            letterSpacing: '0.4px'
          }
        },

        // Label styles (Buttons, Captions)
        labelLarge: {
          value: {
            fontSize: '14px',
            lineHeight: '20px',
            fontWeight: '500',
            letterSpacing: '0.1px'
          }
        },
        labelMedium: {
          value: {
            fontSize: '12px',
            lineHeight: '16px',
            fontWeight: '500',
            letterSpacing: '0.5px'
          }
        },
        labelSmall: {
          value: {
            fontSize: '11px',
            lineHeight: '16px',
            fontWeight: '500',
            letterSpacing: '0.5px'
          }
        }
      }
    },

    // M3 Component Recipes
    recipes: {
      // Button Recipe
      button: {
        className: 'btn',
        description: 'M3 Button styles',
        base: {
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          gap: '8px',
          cursor: 'pointer',
          transition: 'all 0.2s ease-out',
          _disabled: {
            cursor: 'not-allowed'
          }
        },
        variants: {
          variant: {
            primary: {
              height: '40px',
              paddingX: '24px',
              backgroundColor: 'primary',
              color: 'onPrimary',
              fontSize: '14px',
              lineHeight: '20px',
              fontWeight: '500',
              letterSpacing: '0.1px',
              borderRadius: '9999px',
              _hover: {
                backgroundColor: 'primary/90',
                boxShadow: '0 4px 6px -1px rgb(0 0 0 / 0.1)'
              },
              _active: {
                transform: 'scale(0.98)'
              },
              _disabled: {
                backgroundColor: 'onSurface/12',
                color: 'onSurface/38',
                boxShadow: 'none'
              }
            },
            tonal: {
              height: '40px',
              paddingX: '24px',
              backgroundColor: 'secondaryContainer',
              color: 'onSecondaryContainer',
              fontSize: '14px',
              lineHeight: '20px',
              fontWeight: '500',
              letterSpacing: '0.1px',
              borderRadius: '9999px',
              _hover: {
                backgroundColor: 'secondaryContainer/90',
                boxShadow: '0 1px 3px 0 rgb(0 0 0 / 0.1)'
              },
              _active: {
                transform: 'scale(0.98)'
              },
              _disabled: {
                backgroundColor: 'onSurface/12',
                color: 'onSurface/38'
              }
            },
            secondary: {
              height: '40px',
              paddingX: '24px',
              backgroundColor: 'secondaryContainer',
              color: 'onSecondaryContainer',
              fontSize: '14px',
              lineHeight: '20px',
              fontWeight: '500',
              letterSpacing: '0.1px',
              borderRadius: '9999px',
              _hover: {
                backgroundColor: 'secondaryContainer/90',
                boxShadow: '0 1px 3px 0 rgb(0 0 0 / 0.1)'
              },
              _active: {
                transform: 'scale(0.98)'
              },
              _disabled: {
                backgroundColor: 'onSurface/12',
                color: 'onSurface/38'
              }
            },
            outlined: {
              height: '40px',
              paddingX: '24px',
              backgroundColor: 'transparent',
              borderWidth: '1px',
              borderStyle: 'solid',
              borderColor: 'outline',
              color: 'primary',
              fontSize: '14px',
              lineHeight: '20px',
              fontWeight: '500',
              letterSpacing: '0.1px',
              borderRadius: '9999px',
              _hover: {
                backgroundColor: 'primary/8'
              },
              _focus: {
                ringWidth: '2px',
                ringColor: 'primary'
              },
              _active: {
                backgroundColor: 'primary/12'
              },
              _disabled: {
                borderColor: 'onSurface/12',
                color: 'onSurface/38'
              }
            },
            text: {
              height: '40px',
              paddingX: '16px',
              backgroundColor: 'transparent',
              color: 'primary',
              fontSize: '14px',
              lineHeight: '20px',
              fontWeight: '500',
              letterSpacing: '0.1px',
              borderRadius: '9999px',
              minWidth: '64px',
              _hover: {
                backgroundColor: 'primary/8'
              },
              _active: {
                backgroundColor: 'primary/12'
              },
              _disabled: {
                color: 'onSurface/38'
              }
            },
            icon: {
              width: '40px',
              height: '40px',
              borderRadius: '9999px',
              color: 'onSurfaceVariant',
              _hover: {
                backgroundColor: 'onSurfaceVariant/8'
              },
              _active: {
                backgroundColor: 'onSurfaceVariant/12'
              }
            }
          }
        },
        defaultVariants: {
          variant: 'primary'
        }
      },

      // Card Recipe
      card: {
        className: 'card',
        description: 'M3 Card styles',
        base: {
          borderRadius: '12px',
          padding: '16px',
          color: 'onSurface'
        },
        variants: {
          variant: {
            elevated: {
              backgroundColor: 'surfaceContainerLow',
              boxShadow: '0 4px 6px -1px rgb(0 0 0 / 0.1)',
              transition: 'box-shadow 0.2s',
              _hover: {
                boxShadow: '0 10px 15px -3px rgb(0 0 0 / 0.1)'
              }
            },
            filled: {
              backgroundColor: 'surfaceContainerHighest',
              borderWidth: '1px',
              borderStyle: 'solid',
              borderColor: 'transparent'
            },
            outlined: {
              backgroundColor: 'surface',
              borderWidth: '1px',
              borderStyle: 'solid',
              borderColor: 'outlineVariant'
            }
          }
        },
        defaultVariants: {
          variant: 'filled'
        }
      },

      // Input Recipe
      input: {
        className: 'input',
        description: 'M3 Input styles',
        base: {
          width: '100%',
          paddingX: '16px',
          fontSize: '16px',
          lineHeight: '24px',
          fontWeight: '400',
          letterSpacing: '0.5px',
          color: 'onSurface',
          backgroundColor: 'transparent',
          outline: 'none',
          transition: 'all 0.2s',
          _placeholder: {
            color: 'onSurfaceVariant/70'
          }
        },
        variants: {
          variant: {
            filled: {
              height: '56px',
              backgroundColor: 'surfaceContainerHighest',
              borderTopLeftRadius: '8px',
              borderTopRightRadius: '8px',
              borderBottomWidth: '1px',
              borderBottomStyle: 'solid',
              borderBottomColor: 'onSurfaceVariant',
              _hover: {
                backgroundColor: 'surfaceContainerHighest/80'
              },
              _focus: {
                borderBottomWidth: '2px',
                borderBottomColor: 'primary'
              }
            },
            outlined: {
              height: '56px',
              borderWidth: '1px',
              borderStyle: 'solid',
              borderColor: 'outline',
              borderRadius: '8px',
              _focus: {
                borderWidth: '2px',
                borderColor: 'primary',
                color: 'primary'
              }
            },
            default: {
              height: '48px',
              backgroundColor: 'surfaceContainer',
              borderWidth: '1px',
              borderStyle: 'solid',
              borderColor: 'outlineVariant',
              borderRadius: '8px',
              _focus: {
                borderColor: 'primary',
                ringWidth: '1px',
                ringColor: 'primary'
              }
            }
          }
        },
        defaultVariants: {
          variant: 'default'
        }
      }
    },
    slotRecipes: {
      sidebar: sidebarStyles
    }
  },

  // Global CSS
  globalCss: {
    body: {
      backgroundColor: 'background',
      color: 'onSurface',
      fontFamily: 'sans',
      margin: 0,
      WebkitFontSmoothing: 'antialiased',
      MozOsxFontSmoothing: 'grayscale'
    },
    '::selection': {
      backgroundColor: 'primaryContainer',
      color: 'onPrimaryContainer'
    },
    // Scrollbar styling
    '::-webkit-scrollbar': {
      width: '8px',
      height: '8px'
    },
    '::-webkit-scrollbar-track': {
      backgroundColor: 'surfaceContainer'
    },
    '::-webkit-scrollbar-thumb': {
      backgroundColor: 'outlineVariant',
      borderRadius: '9999px'
    },
    '::-webkit-scrollbar-thumb:hover': {
      backgroundColor: 'outline'
    }
  }
})
