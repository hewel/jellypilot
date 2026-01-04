import { Tabs } from '@ark-ui/solid/tabs';
import type { LucideProps } from 'lucide-solid';
import { type Component, For, type JSX } from 'solid-js';
import { css } from 'styled-system/css';
import { sidebar } from 'styled-system/recipes';

export interface SidebarTab {
  id: string;
  label: string;
  icon: Component<LucideProps>;
}

export interface SidebarProps {
  tabs: SidebarTab[];
  value: string;
  onValueChange: (value: string) => void;
  children: JSX.Element;
}

export default function Sidebar(props: SidebarProps) {
  const classes = sidebar();
  return (
    <Tabs.Root
      orientation="vertical"
      value={props.value}
      onValueChange={(details) => props.onValueChange(details.value)}
      class={classes.root}
    >
      {/* Sidebar Navigation */}
      <Tabs.List class={classes.list}>
        <For each={props.tabs}>
          {(tab) => (
            <Tabs.Trigger
              value={tab.id}
              title={tab.label}
              class={classes.trigger}
            >
              <tab.icon
                class={css({
                  width: '24px',
                  height: '24px',
                })}
              />
              <span
                class={css({
                  textStyle: 'labelSmall',
                  fontWeight: 'medium',
                })}
              >
                {tab.label}
              </span>
            </Tabs.Trigger>
          )}
        </For>
      </Tabs.List>

      {/* Main Content Area */}
      <div
        class={css({
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          minWidth: 0,
          height: '100%',
          overflowY: 'auto',
          scrollBehavior: 'smooth',
        })}
      >
        {props.children}
      </div>
    </Tabs.Root>
  );
}

export { Tabs };
