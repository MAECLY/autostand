/**
 * LLM provider configuration: CLI detection, keychain status, connectivity test.
 *
 * API keys only ever travel *into* `store_api_key` — no hook returns, caches,
 * or toasts a key; `useApiKeyStatus` reports presence and origin only.
 */

import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { handleInvokeError } from "@/lib/error";
import { onProviderHealthUpdated, tauriApi } from "@/lib/tauri";
import type { ProviderHealth, ProviderTestMode } from "@/lib/types";

export const llmProvidersKey = ["llm-providers"] as const;
export const providerHealthKey = ["provider-health"] as const;

export function apiKeyStatusKey(provider: string) {
  return ["api-key-status", provider] as const;
}

export function providerModelsKey(provider: string) {
  return ["provider-models", provider] as const;
}

export function cliDetectionKey(provider: string) {
  return ["cli-detection", provider] as const;
}

export function useLlmProviders() {
  return useQuery({
    queryKey: llmProvidersKey,
    queryFn: tauriApi.listLlmProviders,
  });
}

/**
 * Last known provider usage.
 *
 * `get_provider_health` is a pure cache read, so this resolves without probing
 * anything. Live values arrive through `useProviderHealthEvents`.
 */
export function useProviderHealth() {
  return useQuery({
    queryKey: providerHealthKey,
    queryFn: tauriApi.getProviderHealth,
    staleTime: 5 * 60_000,
  });
}

/**
 * Push backend health refreshes into the query cache.
 *
 * The backend has always emitted `provider-health-updated` and `tauri.ts` has
 * always exported the typed helper, but nothing subscribed — so a refresh
 * started by the scheduler or by a compile never repainted Settings. Mount once,
 * near the app shell.
 */
export function useProviderHealthEvents(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    // StrictMode mounts twice: cleanup can run before `listen` resolves, so a
    // late subscription unsubscribes itself instead of leaking.
    let disposed = false;
    let unsubscribe: (() => void) | null = null;

    onProviderHealthUpdated((health: ProviderHealth[]) => {
      queryClient.setQueryData(providerHealthKey, health);
    })
      .then((dispose) => {
        if (disposed) dispose();
        else unsubscribe = dispose;
      })
      .catch((error: unknown) => {
        handleInvokeError(error, "Provider health events");
      });

    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [queryClient]);
}

export function useRefreshProviderHealth() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (provider?: string) => tauriApi.refreshProviderHealth(provider),
    onSuccess: (health) => {
      queryClient.setQueryData(providerHealthKey, health);
    },
    onError: (error) => handleInvokeError(error, "Refresh provider usage"),
  });
}

export interface TestProviderVariables {
  /** `claude` | `ollama` | `openai` | `gemini` | `grok`. */
  provider: string;
  mode: ProviderTestMode;
}

export function useTestProvider() {
  return useMutation({
    mutationFn: ({ provider, mode }: TestProviderVariables) =>
      tauriApi.testLlmProvider(provider, mode),
    onSuccess: (result, { provider, mode }) => {
      const label = `${provider} (${mode})`;
      if (result.ok) {
        toast.success(`${label} — ${result.latency_ms} ms`, {
          description: result.message,
        });
      } else {
        toast.error(`${label} failed`, { description: result.message });
      }
    },
    onError: (error) => handleInvokeError(error, "Test provider"),
  });
}

export interface StoreApiKeyVariables {
  provider: string;
  /** Plaintext key, forwarded straight to the OS keychain. */
  key: string;
}

export function useStoreApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ provider, key }: StoreApiKeyVariables) =>
      tauriApi.storeApiKey(provider, key),
    onSuccess: async (_data, { provider }) => {
      toast.success(`API key stored for ${provider}`);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: apiKeyStatusKey(provider) }),
        queryClient.invalidateQueries({ queryKey: llmProvidersKey }),
        queryClient.invalidateQueries({ queryKey: providerModelsKey(provider) }),
      ]);
    },
    onError: (error) => handleInvokeError(error, "Store API key"),
  });
}

export function useApiKeyStatus(provider: string) {
  return useQuery({
    queryKey: apiKeyStatusKey(provider),
    queryFn: () => tauriApi.getApiKeyStatus(provider),
    enabled: provider.length > 0,
  });
}

export function useDetectCli(provider: string) {
  return useQuery({
    queryKey: cliDetectionKey(provider),
    queryFn: () => tauriApi.detectCli(provider),
    enabled: provider.length > 0,
    // Detection spawns the binary with `--version`; the answer holds for a session.
    staleTime: 5 * 60_000,
  });
}

export function useProviderModels(provider: string) {
  return useQuery({
    queryKey: providerModelsKey(provider),
    queryFn: () => tauriApi.listProviderModels(provider),
    enabled: provider.length > 0,
    // A probe hits the provider API; the catalogue does not change mid-session.
    staleTime: 5 * 60_000,
  });
}
