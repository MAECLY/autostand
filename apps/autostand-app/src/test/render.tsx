/**
 * Render helpers that give every test its own React Query cache.
 *
 * A shared client would let one test's cached data satisfy the next test's
 * query, so the client is built per call and retries are off — a rejected
 * `invoke` must surface as an error immediately, not after three backoffs.
 */

import type { ReactElement, ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  render,
  type RenderOptions,
  type RenderResult,
} from "@testing-library/react";

export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        // No gc timer: nothing is left pending when the test file finishes.
        gcTime: Infinity,
        refetchOnWindowFocus: false,
      },
      mutations: { retry: false },
    },
  });
}

export interface TestProvidersProps {
  children: ReactNode;
}

/** Wrapper for `renderHook`, which takes the component rather than an element. */
export function createWrapper(queryClient: QueryClient) {
  return function TestProviders({ children }: TestProvidersProps) {
    return (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };
}

export interface RenderWithProvidersOptions
  extends Omit<RenderOptions, "wrapper"> {
  /** Pass a pre-seeded client to start the render with cached data. */
  queryClient?: QueryClient;
}

export interface RenderWithProvidersResult extends RenderResult {
  queryClient: QueryClient;
}

export function renderWithProviders(
  ui: ReactElement,
  options: RenderWithProvidersOptions = {},
): RenderWithProvidersResult {
  const { queryClient = createTestQueryClient(), ...rest } = options;
  return {
    ...render(ui, { wrapper: createWrapper(queryClient), ...rest }),
    queryClient,
  };
}
