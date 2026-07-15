import { createListCollection } from '@ark-ui/solid/collection';
import { Select } from '@ark-ui/solid/select';
import { css, cva } from '@styled-system/css';
import { createFileRoute } from '@tanstack/solid-router';
import { ChevronDown } from 'lucide-solid';
import { For, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

const styles = {
  page: css({
    margin: '0 auto',
    maxWidth: '72rem',
    minHeight: '100dvh',
    padding: { base: '1rem', sm: '1.5rem', lg: '2.5rem' },
    width: '100%',
  }),
  header: css({ display: 'grid', gap: '0.75rem', marginBottom: '1.5rem' }),
  eyebrow: css({
    color: 'jellypilot.secondary',
    fontSize: '0.75rem',
    fontWeight: '700',
    letterSpacing: '0.08em',
    lineHeight: '1rem',
    margin: '0',
    textTransform: 'uppercase',
  }),
  title: css({
    color: 'jellypilot.onSurface',
    fontFamily: 'var(--jellypilot-font-display)',
    fontSize: { base: '1.75rem', sm: '2.25rem' },
    fontWeight: '700',
    letterSpacing: '-0.025em',
    lineHeight: '1.1',
    margin: '0',
    textWrap: 'balance',
  }),
  description: css({
    color: 'jellypilot.onSurfaceVariant',
    fontSize: '0.875rem',
    lineHeight: '1.5rem',
    margin: '0',
    maxWidth: '48rem',
    textWrap: 'pretty',
  }),
  viewportReadout: css({
    alignItems: 'center',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.72),
    borderRadius: '9999px',
    color: 'jellypilot.onSurfaceVariant',
    display: 'inline-flex',
    fontSize: '0.75rem',
    fontVariantNumeric: 'tabular-nums',
    fontWeight: '600',
    gap: '0.5rem',
    justifySelf: 'start',
    lineHeight: '1rem',
    padding: '0.5rem 0.75rem',
  }),
  grid: css({
    display: 'grid',
    gap: '1rem',
    gridTemplateColumns: { base: 'minmax(0, 1fr)', sm: 'repeat(2, minmax(0, 1fr))' },
  }),
  card: css({
    background: mix('var(--jellypilot-color-surface-container)', 0.92),
    borderRadius: '1.5rem',
    boxShadow: '0 0 0 1px rgb(255 255 255 / 0.08), 0 18px 40px -28px rgb(0 0 0 / 0.8)',
    display: 'grid',
    gap: '1rem',
    minWidth: '0',
    padding: '1.25rem',
  }),
  cardHeader: css({ display: 'grid', gap: '0.375rem' }),
  cardTitle: css({
    color: 'jellypilot.onSurface',
    fontSize: '1rem',
    fontWeight: '700',
    lineHeight: '1.5rem',
    margin: '0',
    textWrap: 'balance',
  }),
  cardCopy: css({
    color: 'jellypilot.onSurfaceVariant',
    fontSize: '0.75rem',
    lineHeight: '1.25rem',
    margin: '0',
    textWrap: 'pretty',
  }),
  fieldStack: css({ display: 'grid', gap: '1rem' }),
  label: css({
    color: 'jellypilot.onSurfaceVariant',
    display: 'block',
    fontSize: '0.75rem',
    fontWeight: '700',
    letterSpacing: '0.05em',
    lineHeight: '1rem',
    marginBottom: '0.5rem',
    textTransform: 'uppercase',
  }),
  control: css({ alignItems: 'center', display: 'flex', width: '100%' }),
  selectTrigger: cva({
    base: {
      alignItems: 'center',
      background: 'jellypilot.surfaceContainerHigh',
      borderColor: 'jellypilot.outlineVariant',
      borderStyle: 'solid',
      borderWidth: '1px',
      color: 'jellypilot.onSurface',
      cursor: 'pointer',
      display: 'flex',
      fontSize: '0.875rem',
      fontWeight: '500',
      gap: '0.5rem',
      justifyContent: 'space-between',
      lineHeight: '1.25rem',
      minHeight: '2.75rem',
      outline: 'none',
      textAlign: 'left',
      transitionDuration: '200ms',
      transitionProperty: 'background-color, border-color, box-shadow, transform',
      width: '100%',
      _active: { transform: 'scale(0.96)' },
      _disabled: { cursor: 'not-allowed', opacity: '0.5' },
      _focusVisible: {
        borderColor: 'jellypilot.secondary',
        boxShadow: `0 0 0 3px ${mix('var(--jellypilot-color-secondary)', 0.24)}`,
      },
      _hover: { borderColor: 'jellypilot.outline' },
    },
    variants: {
      size: {
        compact: { borderRadius: '0.75rem', minHeight: '2.75rem', paddingInline: '0.75rem' },
        standard: { borderRadius: '1rem', minHeight: '3.25rem', paddingInline: '1rem' },
      },
    },
    defaultVariants: { size: 'standard' },
  }),
  valueText: css({
    minWidth: '0',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  }),
  indicator: css({
    alignItems: 'center',
    display: 'flex',
    flexShrink: '0',
    justifyContent: 'center',
  }),
  indicatorIcon: css({
    color: 'jellypilot.onSurfaceVariant',
    height: '1rem',
    opacity: '0.75',
    transitionDuration: '200ms',
    transitionProperty: 'transform',
    width: '1rem',
    _selectOpen: { transform: 'rotate(180deg)' },
  }),
  positioner: css({ zIndex: '100' }),
  selectContent: css({
    backdropFilter: 'blur(12px)',
    background: mix('var(--jellypilot-color-surface)', 0.96),
    borderRadius: '0.875rem',
    boxShadow: '0 0 0 1px rgb(255 255 255 / 0.1), 0 18px 32px -18px rgb(0 0 0 / 0.9)',
    display: 'grid',
    gap: '0.25rem',
    maxHeight: '15rem',
    overflowY: 'auto',
    padding: '0.5rem',
  }),
  selectItem: css({
    alignItems: 'center',
    borderRadius: '0.625rem',
    color: 'jellypilot.onSurfaceVariant',
    cursor: 'pointer',
    display: 'flex',
    fontSize: '0.875rem',
    justifyContent: 'space-between',
    lineHeight: '1.25rem',
    minHeight: '2.5rem',
    outline: 'none',
    padding: '0.625rem 0.75rem',
    transitionDuration: '150ms',
    transitionProperty: 'background-color, color',
    '&[data-highlighted]': {
      background: 'jellypilot.surfaceContainerHighest',
      color: 'jellypilot.onSurface',
    },
    '&[data-disabled]': { cursor: 'not-allowed', opacity: '0.5' },
  }),
  statusPanel: css({
    alignItems: 'center',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.62),
    borderRadius: '1rem',
    display: 'flex',
    gap: '0.75rem',
    minHeight: '4.5rem',
    padding: '0.875rem 1rem',
  }),
  statusDot: cva({
    base: {
      borderRadius: '9999px',
      boxShadow: '0 0 0 4px color-mix(in srgb, currentColor 14%, transparent)',
      flexShrink: '0',
      height: '0.75rem',
      width: '0.75rem',
    },
    variants: {
      state: {
        connected: { background: 'jellypilot.tertiary', color: 'jellypilot.tertiary' },
        disconnected: { background: 'jellypilot.error', color: 'jellypilot.error' },
      },
    },
  }),
  statusPulse: css({
    animation: 'pandaPrototypePulse 1.6s cubic-bezier(0.2, 0, 0, 1) infinite',
    _motionReduce: { animation: 'none' },
  }),
  statusCopy: css({ display: 'grid', flex: '1', gap: '0.125rem', minWidth: '0' }),
  statusLabel: css({
    color: 'jellypilot.onSurface',
    fontSize: '0.875rem',
    fontWeight: '700',
    lineHeight: '1.25rem',
  }),
  statusDetail: css({
    color: 'jellypilot.onSurfaceVariant',
    fontSize: '0.75rem',
    lineHeight: '1rem',
  }),
  toggleButton: css({
    background: 'transparent',
    borderColor: 'jellypilot.outlineVariant',
    borderRadius: '0.75rem',
    borderStyle: 'solid',
    borderWidth: '1px',
    color: 'jellypilot.onSurface',
    cursor: 'pointer',
    fontSize: '0.75rem',
    fontWeight: '700',
    minHeight: '2.5rem',
    paddingInline: '0.875rem',
    transitionDuration: '150ms',
    transitionProperty: 'background-color, border-color, transform',
    _active: { transform: 'scale(0.96)' },
    _focusVisible: {
      borderColor: 'jellypilot.secondary',
      outline: '2px solid var(--jellypilot-color-secondary)',
      outlineOffset: '2px',
    },
    _hover: {
      background: 'jellypilot.surfaceContainerHighest',
      borderColor: 'jellypilot.outline',
    },
  }),
  stateDump: css({
    background: mix('var(--jellypilot-color-background)', 0.72),
    borderRadius: '0.75rem',
    color: 'jellypilot.onSurfaceVariant',
    fontFamily: 'var(--jellypilot-font-mono)',
    fontSize: '0.6875rem',
    lineHeight: '1.125rem',
    margin: '0',
    overflowX: 'auto',
    padding: '0.75rem',
    whiteSpace: 'pre-wrap',
  }),
};

interface PrototypeSelectProps {
  label: string;
  size: 'compact' | 'standard';
  value: () => string;
  onValueChange: (value: string) => void;
}

const selectItems = [
  { label: 'English — Stereo', value: 'eng-stereo' },
  { label: 'Japanese — 5.1', value: 'jpn-surround' },
  { disabled: true, label: 'Commentary — unavailable', value: 'commentary' },
];

function PrototypeSelect(props: PrototypeSelectProps) {
  const collection = createMemo(() => createListCollection({ items: selectItems }));

  return (
    <Select.Root
      collection={collection()}
      closeOnSelect
      value={[props.value()]}
      onValueChange={(details) => {
        const value = details.value[0];
        const item = selectItems.find((candidate) => candidate.value === value);
        if (item && !item.disabled) props.onValueChange(item.value);
      }}
      positioning={{ sameWidth: true }}
    >
      <Select.Label class={styles.label}>{props.label}</Select.Label>
      <Select.Control class={styles.control}>
        <Select.Trigger class={styles.selectTrigger({ size: props.size })}>
          <Select.ValueText class={styles.valueText} />
          <Select.Indicator class={styles.indicator}>
            <ChevronDown class={styles.indicatorIcon} />
          </Select.Indicator>
        </Select.Trigger>
      </Select.Control>
      <Portal>
        <Select.Positioner class={styles.positioner}>
          <Select.Content class={styles.selectContent}>
            <For each={collection().items}>
              {(item) => (
                <Select.Item item={item} class={styles.selectItem}>
                  <Select.ItemText>{item.label}</Select.ItemText>
                </Select.Item>
              )}
            </For>
          </Select.Content>
        </Select.Positioner>
      </Portal>
      <Select.HiddenSelect />
    </Select.Root>
  );
}

export function PandaPrototypePage() {
  const [standardTrack, setStandardTrack] = createSignal('eng-stereo');
  const [compactTrack, setCompactTrack] = createSignal('jpn-surround');
  const [connected, setConnected] = createSignal(true);
  const [viewportWidth, setViewportWidth] = createSignal(window.innerWidth);

  onMount(() => {
    const updateViewportWidth = () => setViewportWidth(window.innerWidth);
    window.addEventListener('resize', updateViewportWidth);
    onCleanup(() => window.removeEventListener('resize', updateViewportWidth));
  });

  return (
    <main data-panda-prototype class={styles.page}>
      <header class={styles.header}>
        <p class={styles.eyebrow}>Disposable integration fixture</p>
        <h1 class={styles.title}>Panda CSS hard-case prototype</h1>
        <p class={styles.description}>
          This route exercises JellyPilot tokens, responsive extraction, recipes, Ark state hooks,
          generated-parent selectors, global CSS, keyframes, and Solid class updates in WebKit.
        </p>
        <output class={styles.viewportReadout} aria-label="Current viewport width">
          WebView viewport: {viewportWidth()}px
        </output>
      </header>

      <div class={styles.grid}>
        <section class={styles.card} aria-labelledby="panda-select-title">
          <div class={styles.cardHeader}>
            <h2 id="panda-select-title" class={styles.cardTitle}>
              Ark Select states
            </h2>
            <p class={styles.cardCopy}>
              Compare standard and compact recipe variants, then inspect focus, disabled options,
              portal placement, and the open-state chevron.
            </p>
          </div>
          <div class={styles.fieldStack}>
            <PrototypeSelect
              label="Standard audio track"
              size="standard"
              value={standardTrack}
              onValueChange={setStandardTrack}
            />
            <PrototypeSelect
              label="Compact subtitle track"
              size="compact"
              value={compactTrack}
              onValueChange={setCompactTrack}
            />
          </div>
          <pre class={styles.stateDump} aria-label="Select state">
            {JSON.stringify(
              { compactTrack: compactTrack(), standardTrack: standardTrack() },
              null,
              2,
            )}
          </pre>
        </section>

        <section class={styles.card} aria-labelledby="panda-status-title">
          <div class={styles.cardHeader}>
            <h2 id="panda-status-title" class={styles.cardTitle}>
              Reactive status recipe
            </h2>
            <p class={styles.cardCopy}>
              Toggle the state to exercise a runtime recipe variant and a conditional Solid
              classList animation without constructing a class string.
            </p>
          </div>
          <div class={styles.statusPanel}>
            <span
              aria-hidden="true"
              class={styles.statusDot({ state: connected() ? 'connected' : 'disconnected' })}
              classList={{ [styles.statusPulse]: connected() }}
            />
            <div class={styles.statusCopy} aria-live="polite">
              <span class={styles.statusLabel}>{connected() ? 'Connected' : 'Disconnected'}</span>
              <span class={styles.statusDetail}>
                {connected() ? 'Panda animation active' : 'Static failure state'}
              </span>
            </div>
            <button
              type="button"
              class={styles.toggleButton}
              aria-pressed={connected()}
              onClick={() => setConnected((value) => !value)}
            >
              Toggle
            </button>
          </div>
          <pre class={styles.stateDump} aria-label="Connection state">
            {JSON.stringify({ connected: connected() }, null, 2)}
          </pre>
        </section>
      </div>
    </main>
  );
}

export const Route = createFileRoute('/panda-prototype')({
  component: PandaPrototypePage,
});
