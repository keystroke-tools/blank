import { Button as BaseButton } from '@base-ui/react/button'
import type { ComponentProps } from 'react'

type Props = ComponentProps<typeof BaseButton> & { tone?: 'primary' | 'quiet' }

export function Button({ className = '', tone = 'primary', ...props }: Props) {
	const colors =
		tone === 'primary'
			? 'bg-primary text-primary-ink hover:bg-[#f5ffc2]'
			: 'border border-border bg-surface text-ink hover:bg-surface-muted'
	return (
		<BaseButton
			className={`inline-flex h-10 items-center justify-center px-4 text-sm font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-50 ${colors} ${className}`}
			{...props}
		/>
	)
}
