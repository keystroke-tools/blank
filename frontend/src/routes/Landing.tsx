import { Link } from '@tanstack/react-router'

export function Landing({ setupRequired }: { setupRequired: boolean }) {
	return (
		<div className="min-h-screen overflow-hidden">
			<PublicHeader setupRequired={setupRequired} />
			<main>
				<section className="mx-auto grid max-w-6xl gap-14 px-6 py-20 lg:grid-cols-[1.1fr_0.9fr] lg:items-center lg:py-28">
					<div>
						<p className="text-xs font-semibold uppercase tracking-[0.2em] text-primary">
							Self-hosted static deployments
						</p>
						<h1 className="mt-5 max-w-3xl text-5xl font-semibold leading-[1.02] tracking-[-0.045em] sm:text-6xl">
							Push a repository.
							<br />
							Blank handles the rest.
						</h1>
						<p className="mt-7 max-w-xl text-base leading-7 text-muted">
							A small deployment platform for static frontends.
							Automatic builds, managed TLS, atomic releases,
							rollbacks, GitHub integration, and useful request
							analytics.
						</p>
						<div className="mt-9 flex flex-wrap gap-3">
							<Link
								to={setupRequired ? '/setup' : '/login'}
								className="inline-flex h-11 items-center bg-primary px-5 text-sm font-semibold text-primary-ink"
							>
								{setupRequired
									? 'Set up Blank'
									: 'Open dashboard'}
							</Link>
							<Link
								to="/docs"
								className="inline-flex h-11 items-center border border-border bg-surface px-5 text-sm font-semibold hover:border-primary"
							>
								Read the docs
							</Link>
						</div>
					</div>
					<div className="border border-border bg-surface p-3 shadow-[0_24px_80px_rgba(0,0,0,0.25)]">
						<div className="flex items-center gap-2 border-b border-border px-3 py-3">
							<span className="size-2 rounded-full bg-danger" />
							<span className="size-2 rounded-full bg-primary" />
							<span className="ml-2 font-mono text-[11px] text-muted">
								blank / deployment
							</span>
						</div>
						<div className="space-y-3 bg-[#111] p-5 font-mono text-xs leading-6 text-[#d9d9d9]">
							<p className="text-muted">$ git push origin main</p>
							<p>
								<span className="text-primary">[fetching]</span>{' '}
								Resolved 8c4f21b
							</p>
							<p>
								<span className="text-primary">[building]</span>{' '}
								pnpm run build
							</p>
							<p>
								<span className="text-primary">
									[publishing]
								</span>{' '}
								dist/ → immutable release
							</p>
							<p>
								<span className="text-primary">[success]</span>{' '}
								Site activated atomically
							</p>
						</div>
					</div>
				</section>
				<section className="border-y border-border bg-surface/40">
					<div className="mx-auto grid max-w-6xl gap-px bg-border sm:grid-cols-3">
						<Feature
							title="Repository aware"
							text="Detects common Node tooling, project directories, dependencies, build commands, and output folders."
						/>
						<Feature
							title="Safe releases"
							text="Builds isolated checkouts and switches releases atomically. Previous successful releases remain available for rollback."
						/>
						<Feature
							title="One small service"
							text="Actix, SQLite, Mise, Git, and embedded Chimney keep the operational surface deliberately compact."
						/>
					</div>
				</section>
			</main>
			<PublicFooter />
		</div>
	)
}

export function Docs() {
	return (
		<div className="min-h-screen">
			<PublicHeader setupRequired={false} />
			<main className="mx-auto max-w-4xl px-6 py-16">
				<p className="text-xs font-semibold uppercase tracking-[0.2em] text-primary">
					Documentation
				</p>
				<h1 className="mt-3 text-4xl font-semibold tracking-tight">
					Blank in five minutes
				</h1>
				<p className="mt-4 max-w-2xl text-sm leading-7 text-muted">
					Connect a repository, review Blank's detected build
					settings, add a domain, and deploy. Successful releases are
					served immediately by Chimney.
				</p>
				<div className="mt-12 grid gap-6">
					<DocStep number="01" title="Connect a repository">
						Use a Git URL or connect the per-instance GitHub App.
						GitHub App installations can browse and fetch private
						repositories without permanent personal tokens.
					</DocStep>
					<DocStep number="02" title="Confirm the build">
						Blank detects common package managers and frontend
						frameworks. Adjust dependencies, install command, build
						command, project directory, and publish directory when
						needed.
					</DocStep>
					<DocStep number="03" title="Configure domains">
						Add the apex domain and Blank can suggest its www
						counterpart. DNS checks compare resolved addresses with
						BLANK_EXPECTED_IPS.
					</DocStep>
					<DocStep number="04" title="Deploy and observe">
						Deployments run in isolated worktrees. Follow live logs,
						retry failures, roll back old releases, and inspect
						paginated request analytics.
					</DocStep>
				</div>
				<section className="mt-12 border border-border bg-surface p-6">
					<h2 className="font-semibold">Important server settings</h2>
					<pre className="mt-4 overflow-x-auto bg-[#111] p-4 font-mono text-xs leading-6 text-[#d9d9d9]">
						BLANK_PUBLIC_URL=https://blank.example.com{`\n`}
						BLANK_EXPECTED_IPS=203.0.113.10{`\n`}
						BLANK_SECURE_COOKIES=true
					</pre>
					<p className="mt-4 text-sm text-muted">
						The interactive installer and updater prompt for missing
						runtime settings.
					</p>
				</section>
			</main>
			<PublicFooter />
		</div>
	)
}

function PublicHeader({ setupRequired }: { setupRequired: boolean }) {
	return (
		<header className="border-b border-border">
			<div className="mx-auto flex h-16 max-w-6xl items-center justify-between px-6">
				<Link to="/" className="flex items-center gap-3">
					<span className="grid size-8 place-items-center bg-primary text-xs font-black text-primary-ink">
						B
					</span>
					<span className="font-semibold">Blank</span>
				</Link>
				<nav className="flex items-center gap-5 text-sm">
					<Link to="/docs" className="text-muted hover:text-ink">
						Docs
					</Link>
					<Link
						to={setupRequired ? '/setup' : '/login'}
						className="font-semibold text-primary"
					>
						{setupRequired ? 'Set up' : 'Sign in'}
					</Link>
				</nav>
			</div>
		</header>
	)
}
function PublicFooter() {
	return (
		<footer className="mx-auto flex max-w-6xl flex-wrap justify-between gap-3 px-6 py-10 text-xs text-muted">
			<span>Blank. Small, self-hosted deployments.</span>
			<span>Developed with assistance from large language models.</span>
		</footer>
	)
}
function Feature({ title, text }: { title: string; text: string }) {
	return (
		<article className="bg-background p-7">
			<h2 className="font-semibold">{title}</h2>
			<p className="mt-3 text-sm leading-6 text-muted">{text}</p>
		</article>
	)
}
function DocStep({
	number,
	title,
	children,
}: {
	number: string
	title: string
	children: string
}) {
	return (
		<section className="grid gap-3 border-t border-border pt-6 sm:grid-cols-[4rem_1fr]">
			<span className="font-mono text-xs text-primary">{number}</span>
			<div>
				<h2 className="font-semibold">{title}</h2>
				<p className="mt-2 text-sm leading-7 text-muted">{children}</p>
			</div>
		</section>
	)
}
