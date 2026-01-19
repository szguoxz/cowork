import { forwardRef, ButtonHTMLAttributes } from 'react'

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'default' | 'destructive' | 'outline' | 'secondary' | 'ghost' | 'link' | 'gradient'
  size?: 'default' | 'sm' | 'lg' | 'icon'
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className = '', variant = 'default', size = 'default', ...props }, ref) => {
    const baseStyles = `
      inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-lg text-sm font-medium
      ring-offset-background transition-all duration-200
      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:ring-offset-2
      disabled:pointer-events-none disabled:opacity-50
      active:scale-[0.98]
    `

    const variants = {
      default: 'bg-primary text-primary-foreground hover:bg-primary/90 shadow-glow-sm hover:shadow-glow-md',
      destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
      outline: 'border border-border bg-transparent hover:bg-white/5 hover:border-border-hover text-foreground',
      secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
      ghost: 'hover:bg-white/5 hover:text-foreground text-muted-foreground',
      link: 'text-primary underline-offset-4 hover:underline',
      gradient: `
        relative overflow-hidden text-white
        bg-gradient-to-r from-violet-500 to-purple-600
        hover:from-violet-400 hover:to-purple-500
        shadow-glow-sm hover:shadow-glow-md
      `,
    }

    const sizes = {
      default: 'h-10 px-4 py-2',
      sm: 'h-9 rounded-md px-3 text-xs',
      lg: 'h-11 rounded-lg px-8',
      icon: 'h-10 w-10',
    }

    return (
      <button
        ref={ref}
        className={`${baseStyles} ${variants[variant]} ${sizes[size]} ${className}`}
        {...props}
      />
    )
  }
)

Button.displayName = 'Button'

export { Button }
