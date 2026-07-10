import {
  createContext,
  createEffect,
  createSignal,
  onCleanup,
  useContext,
  type ParentProps,
} from 'solid-js';

import {
  createConfigCoordinator,
  type ConfigCoordinator,
  type ConfigCoordinatorState,
} from './configCoordinator';

const ConfigCoordinatorContext = createContext<{
  coordinator: ConfigCoordinator;
  state: () => ConfigCoordinatorState;
} | null>(null);

export function ConfigCoordinatorProvider(props: ParentProps) {
  const coordinator = createConfigCoordinator();
  const [state, setState] = createSignal(coordinator.getState());

  createEffect(() => {
    const unsubscribe = coordinator.subscribe(setState);
    void coordinator.bootstrap();
    onCleanup(unsubscribe);
  });

  return (
    <ConfigCoordinatorContext.Provider
      value={{
        coordinator,
        state,
      }}
    >
      {props.children}
    </ConfigCoordinatorContext.Provider>
  );
}

export function useConfigCoordinator() {
  const value = useContext(ConfigCoordinatorContext);
  if (!value) {
    throw new Error('useConfigCoordinator requires ConfigCoordinatorProvider');
  }
  return value;
}
