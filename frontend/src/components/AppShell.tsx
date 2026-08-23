import { useMutation, useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { api } from '../lib/api'
import { queryClient, queryKeys } from '../lib/query-client'
import { Button } from './Button'

export function AppShell({ children }: { children: ReactNode }) {
	const { data } = useQuery({
		queryKey: queryKeys.auth,
		queryFn: api.authStatus,
	})
	const logout = useMutation({
		mutationFn: () => api.logout(data?.csrf_token ?? ''),
		onSuccess: () =>
			queryClient.invalidateQueries({ queryKey: queryKeys.auth }),
	})
	return (
		<div className="min-h-screen">
			<header className="border-b border-border">
				<div className="mx-auto flex h-16 max-w-6xl items-center justify-between px-6">
					<Link to="/" className="flex items-center gap-3">
						<span className="grid size-8 place-items-center bg-primary text-xs font-black text-primary-ink">
							B
						</span>
						<span className="font-semibold">Blank</span>
					</Link>
					<div className="flex items-center gap-4">
						<span className="hidden text-sm text-muted sm:block">
							{data?.identifier}
						</span>
						<Button tone="quiet" onClick={() => logout.mutate()}>
							Sign out
						</Button>
					</div>
				</div>
			</header>
			{children}
		</div>
	)
}
