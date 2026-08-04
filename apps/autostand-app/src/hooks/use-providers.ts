/**
 * LLM provider configuration: CLI detection, keychain status, connectivity test.
 *
 * API keys only ever travel *into* `store_api_key` — no hook returns, caches,
 * or toasts a key; `useApiKeyStatus` reports presence and origin only.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";
import type { ProviderTestMode } from "@/lib/types";

export const llmProvidersKey = ["llm-providers"] as const;

export function apiKeyStatusKey(provider: string) {
  return ["api-key-status", provider] as const;
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
