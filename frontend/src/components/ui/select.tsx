import { forwardRef, SelectHTMLAttributes } from 'react'

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {}

const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ className = '', children, ...props }, ref) => {
    return (
      <select
        className={`
          flex h-10 w-full rounded-lg border border-border bg-secondary/50 px-3 py-2 text-sm
          text-foreground ring-offset-background transition-all duration-200
          hover:border-border-hover
          focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:border-primary/50
          disabled:cursor-not-allowed disabled:opacity-50
          [&>option]:bg-card [&>option]:text-foreground
          ${className}
        `}
        ref={ref}
        {...props}
      >
        {children}
      </select>
    )
  }
)
Select.displayName = 'Select'

export { Select }
