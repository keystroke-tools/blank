import { QueryClient } from '@tanstack/react-query'

export const queryKeys = {
	auth: ['auth'] as const,
	health: ['health'] as const,
	sites: ['sites'] as const,
	site: (id: string) => ['sites', id] as const,
	deployKey: (id: string) => ['sites', id, 'deploy-key'] as const,
	configuration: (id: string) => ['sites', id, 'configuration'] as const,
	deployments: (id: string) => ['sites', id, 'deployments'] as const,
	deployment: (id: string) => ['deployments', id] as const,
}

export const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			staleTime: 15_000,
			retry: false,
			refetchOnWindowFocus: false,
		},
		mutations: { retry: false },
	},
})
