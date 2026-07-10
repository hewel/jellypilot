import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { RouterProvider } from '@tanstack/solid-router';

import { ConfigGate } from './components/ConfigGate';
import { ToastProvider } from './components/ToastProvider';
import { appLinkAdapter, router } from './router';

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

const App = () => (
  <QueryClientProvider client={queryClient}>
    <ConfigGate linkAdapter={appLinkAdapter}>
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    </ConfigGate>
  </QueryClientProvider>
);

export default App;
