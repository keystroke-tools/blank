import { useForm } from '@tanstack/react-form'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { AppShell } from '../components/AppShell'
import { Button } from '../components/Button'
import { api } from '../lib/api'
import { queryClient, queryKeys } from '../lib/query-client'

const fieldClass =
	'mt-2 h-11 w-full border border-border bg-background px-3 outline-none transition focus:border-primary'

export function Users() {
	const auth = useQuery({
		queryKey: queryKeys.auth,
		queryFn: api.authStatus,
	})
	const administrators = useQuery({
		queryKey: queryKeys.administrators,
		queryFn: api.administrators,
	})
	const [formError, setFormError] = useState<string | null>(null)
	const [notice, setNotice] = useState<string | null>(null)
	const mutation = useMutation({
		mutationFn: (value: { identifier: string; password: string }) =>
			api.createAdministrator(value, auth.data?.csrf_token ?? ''),
		onSuccess: async (administrator) => {
			await queryClient.invalidateQueries({
				queryKey: queryKeys.administrators,
			})
			form.reset()
			setFormError(null)
			setNotice(`${administrator.identifier} can now sign in.`)
		},
	})
	const form = useForm({
		defaultValues: {
			identifier: '',
			password: '',
			confirmation: '',
		},
		onSubmit: ({ value }) => {
			setFormError(null)
			setNotice(null)
			if (value.identifier.trim().length < 3) {
				setFormError('Enter an identifier with at least 3 characters.')
				return
			}
			if (value.password.length < 12) {
				setFormError(
					'The password must contain at least 12 characters.',
				)
				return
			}
			if (value.password !== value.confirmation) {
				setFormError('The passwords do not match.')
				return
			}
			mutation.mutate({
				identifier: value.identifier.trim(),
				password: value.password,
			})
		},
	})

	return (
		<AppShell>
			<main className="mx-auto max-w-5xl px-6 py-12">
				<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted">
					Access
				</p>
				<h1 className="mt-2 text-3xl font-semibold tracking-tight">
					Administrators
				</h1>
				<p className="mt-2 max-w-2xl text-sm leading-6 text-muted">
					Every administrator has full access to sites, deployments,
					analytics, integrations, and user management.
				</p>

				<div className="mt-10 grid gap-8 lg:grid-cols-[1fr_1fr] lg:items-start">
					<section className="border border-border bg-surface p-6">
						<h2 className="font-semibold">Current users</h2>
						{administrators.isPending && (
							<p className="mt-5 text-sm text-muted">
								Loading users…
							</p>
						)}
						{administrators.isError && (
							<p className="mt-5 text-sm text-danger">
								{administrators.error.message}
							</p>
						)}
						{administrators.data && (
							<div className="mt-5 divide-y divide-border border-y border-border">
								{administrators.data.map((administrator) => (
									<div
										key={administrator.id}
										className="flex items-center justify-between gap-4 py-4"
									>
										<div className="min-w-0">
											<p className="truncate text-sm font-semibold">
												{administrator.identifier}
											</p>
											<p className="mt-1 text-xs text-muted">
												Added{' '}
												{new Date(
													`${administrator.created_at}Z`,
												).toLocaleDateString()}
											</p>
										</div>
										{administrator.is_current && (
											<span className="border border-primary/40 px-2 py-1 text-[0.625rem] font-semibold uppercase tracking-wide text-primary">
												You
											</span>
										)}
									</div>
								))}
							</div>
						)}
					</section>

					<section className="border border-border bg-surface p-6">
						<h2 className="font-semibold">Add administrator</h2>
						<p className="mt-2 text-xs leading-5 text-muted">
							Create credentials for someone who should have full
							access to this Blank instance.
						</p>
						<form
							className="mt-6 space-y-5"
							onSubmit={(event) => {
								event.preventDefault()
								event.stopPropagation()
								void form.handleSubmit()
							}}
							noValidate
						>
							<form.Field name="identifier">
								{(field) => (
									<label className="block text-sm font-medium">
										Username or email
										<input
											autoComplete="username"
											value={field.state.value}
											onChange={(event) =>
												field.handleChange(
													event.target.value,
												)
											}
											className={fieldClass}
										/>
									</label>
								)}
							</form.Field>
							<form.Field name="password">
								{(field) => (
									<label className="block text-sm font-medium">
										Password
										<input
											type="password"
											autoComplete="new-password"
											value={field.state.value}
											onChange={(event) =>
												field.handleChange(
													event.target.value,
												)
											}
											className={fieldClass}
										/>
										<span className="mt-2 block text-xs text-muted">
											At least 12 characters.
										</span>
									</label>
								)}
							</form.Field>
							<form.Field name="confirmation">
								{(field) => (
									<label className="block text-sm font-medium">
										Confirm password
										<input
											type="password"
											autoComplete="new-password"
											value={field.state.value}
											onChange={(event) =>
												field.handleChange(
													event.target.value,
												)
											}
											className={fieldClass}
										/>
									</label>
								)}
							</form.Field>
							{(formError || mutation.error) && (
								<p role="alert" className="text-sm text-danger">
									{formError ?? mutation.error?.message}
								</p>
							)}
							{notice && (
								<p
									role="status"
									className="text-sm text-primary"
								>
									{notice}
								</p>
							)}
							<Button type="submit" disabled={mutation.isPending}>
								{mutation.isPending
									? 'Adding…'
									: 'Add administrator'}
							</Button>
						</form>
					</section>
				</div>
			</main>
		</AppShell>
	)
}
