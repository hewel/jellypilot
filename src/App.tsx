import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { RouterProvider } from '@tanstack/solid-router';

import { ConfigGate } from './components/ConfigGate';
import { ToastProvider } from './components/ToastProvider';
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

const App = () => (
  <QueryClientProvider client={queryClient}>
    <ToastProvider>
      <ConfigGate>
        <RouterProvider router={router} />
      </ConfigGate>
    </ToastProvider>
  </QueryClientProvider>
);

export default App;
