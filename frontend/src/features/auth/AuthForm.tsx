import { useMutation } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { type FormEvent, useState } from 'react'
import { Button } from '../../components/Button'
import { api } from '../../lib/api'
import { queryClient, queryKeys } from '../../lib/query-client'

export function AuthForm({ mode }: { mode: 'setup' | 'login' }) {
	const navigate = useNavigate()
	const [identifier, setIdentifier] = useState('')
	const [password, setPassword] = useState('')
	const [validationError, setValidationError] = useState<string | null>(null)
	const mutation = useMutation({
		mutationFn: () =>
			mode === 'setup'
				? api.setup({ identifier, password })
				: api.login({ identifier, password }),
		onSuccess: async (status) => {
			queryClient.setQueryData(queryKeys.auth, status)
			await queryClient.invalidateQueries({ queryKey: queryKeys.sites })
			await navigate({ to: '/', replace: true })
		},
	})

	function submit(event: FormEvent) {
		event.preventDefault()
		setValidationError(null)
		if (identifier.trim().length < 3) {
			setValidationError(
				'Enter a username or email with at least 3 characters.',
			)
			return
		}
		if (password.length < 12) {
			setValidationError(
				'Your password must contain at least 12 characters.',
			)
			return
		}
		mutation.mutate()
	}

	return (
		<main className="grid min-h-screen place-items-center px-5 py-12">
			<section className="w-full max-w-sm">
				<div className="mb-10 flex items-center gap-3">
					<span className="grid size-9 place-items-center bg-primary text-sm font-black text-primary-ink">
						B
					</span>
					<span className="text-lg font-semibold tracking-tight">
						Blank
					</span>
				</div>
				<p className="mb-2 text-xs font-semibold uppercase tracking-[0.18em] text-primary">
					{mode === 'setup' ? 'First run' : 'Administrator'}
				</p>
				<h1 className="text-3xl font-semibold tracking-tight">
					{mode === 'setup'
						? 'Create your administrator'
						: 'Welcome back'}
				</h1>
				<p className="mt-3 text-sm leading-6 text-muted">
					{mode === 'setup'
						? 'One account is all Blank needs. Choose a strong password to finish setup.'
						: 'Sign in to manage sites and deployments.'}
				</p>
				<form className="mt-8 space-y-5" onSubmit={submit} noValidate>
					<label className="block text-sm font-medium">
						Username or email
						<input
							autoComplete="username"
							value={identifier}
							onChange={(e) => setIdentifier(e.target.value)}
							className="mt-2 h-11 w-full border border-border bg-surface px-3 outline-none transition focus:border-primary"
						/>
					</label>
					<label className="block text-sm font-medium">
						Password
						<input
							type="password"
							autoComplete={
								mode === 'setup'
									? 'new-password'
									: 'current-password'
							}
							value={password}
							onChange={(e) => setPassword(e.target.value)}
							className="mt-2 h-11 w-full border border-border bg-surface px-3 outline-none transition focus:border-primary"
						/>
						{mode === 'setup' && (
							<span className="mt-2 block text-xs text-muted">
								At least 12 characters.
							</span>
						)}
					</label>
					{(validationError || mutation.error) && (
						<p
							role="alert"
							className="border-l-2 border-danger pl-3 text-sm text-danger"
						>
							{validationError ?? mutation.error?.message}
						</p>
					)}
					<Button
						type="submit"
						className="w-full"
						disabled={mutation.isPending}
					>
						{mutation.isPending
							? 'Working…'
							: mode === 'setup'
								? 'Create administrator'
								: 'Sign in'}
					</Button>
				</form>
			</section>
		</main>
	)
}
