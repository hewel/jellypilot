import { PopupRoot } from '@components/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { RouterProvider } from '@tanstack/solid-router';

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
      <PopupRoot>
        <RouterProvider router={router} />
      </PopupRoot>
    </ToastProvider>
  </QueryClientProvider>
);

export default App;
