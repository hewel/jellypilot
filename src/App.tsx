import { AppearanceProvider } from '@components/AppearanceProvider';
import { ToastProvider } from '@components/ToastProvider';
import { PopupRoot } from '@components/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { RouterProvider } from '@tanstack/solid-router';
import type { BootstrappedAppearance } from '~effects/appearance';

import { router } from './router';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: false,
    },
    mutations: {
      retry: false,
    },
  },
});

const App = (props: { readonly initialAppearance: BootstrappedAppearance }) => (
  <QueryClientProvider client={queryClient}>
    <ToastProvider>
      <AppearanceProvider initial={props.initialAppearance}>
        <PopupRoot>
          <RouterProvider router={router} />
        </PopupRoot>
      </AppearanceProvider>
    </ToastProvider>
  </QueryClientProvider>
);

export default App;
