import { HTMLAttributes } from 'react'

export interface BadgeProps extends HTMLAttributes<HTMLDivElement> {
  variant?: 'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning'
}

function Badge({ className = '', variant = 'default', ...props }: BadgeProps) {
  const variants = {
    default: 'border-transparent bg-primary/20 text-primary',
    secondary: 'border-transparent bg-secondary text-secondary-foreground',
    destructive: 'border-transparent bg-error/20 text-error',
    outline: 'text-foreground border-border',
    success: 'border-transparent bg-success/20 text-success',
    warning: 'border-transparent bg-warning/20 text-warning',
  }

  return (
    <div
      className={`
        inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium
        transition-colors focus:outline-none focus:ring-2 focus:ring-primary/50 focus:ring-offset-2
        ${variants[variant]} ${className}
      `}
      {...props}
    />
  )
}

export { Badge }
