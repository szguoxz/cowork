import { useState, useEffect, useCallback } from 'react'
import { HelpCircle, Check } from 'lucide-react'
import type { QuestionData } from '../bindings'

interface QuestionModalProps {
  requestId: string
  questions: QuestionData[]
  onAnswer: (requestId: string, answers: Record<string, string>) => void
}

export default function QuestionModal({ requestId, questions, onAnswer }: QuestionModalProps) {
  const [currentIdx, setCurrentIdx] = useState(0)
  const [answers, setAnswers] = useState<Record<string, string>>({})
  const [customInput, setCustomInput] = useState('')
  const [showOther, setShowOther] = useState(false)

  const question = questions[currentIdx]
  if (!question) return null

  const selectedAnswer = answers[currentIdx.toString()]

  const selectOption = useCallback((label: string) => {
    setShowOther(false)
    setCustomInput('')
    setAnswers(prev => ({ ...prev, [currentIdx.toString()]: label }))
  }, [currentIdx])

  const selectOther = useCallback(() => {
    setShowOther(true)
    setAnswers(prev => ({ ...prev, [currentIdx.toString()]: customInput || 'Other' }))
  }, [currentIdx, customInput])

  const submit = useCallback(() => {
    // If showing "Other" with custom input, use that value
    const finalAnswers = { ...answers }
    if (showOther && customInput) {
      finalAnswers[currentIdx.toString()] = customInput
    }

    if (currentIdx < questions.length - 1) {
      // Move to next question
      setCurrentIdx(prev => prev + 1)
      setShowOther(false)
      setCustomInput('')
    } else {
      // Submit all answers
      onAnswer(requestId, finalAnswers)
    }
  }, [answers, currentIdx, questions.length, requestId, onAnswer, showOther, customInput])

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't capture if typing in the custom input
      if (showOther && e.target instanceof HTMLInputElement) return

      // Number keys 1-4 to select options
      if (e.key >= '1' && e.key <= '9') {
        const idx = parseInt(e.key) - 1
        if (idx < question.options.length) {
          e.preventDefault()
          selectOption(question.options[idx].label)
        } else if (idx === question.options.length) {
          e.preventDefault()
          selectOther()
        }
      }

      // Enter to submit/next
      if (e.key === 'Enter' && !e.shiftKey && selectedAnswer) {
        e.preventDefault()
        submit()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [question, selectOption, selectOther, submit, selectedAnswer, showOther])

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/50" />

      {/* Modal */}
      <div className="relative w-[520px] max-w-[90vw] bg-card border border-border rounded-xl shadow-2xl">
        {/* Header */}
        <div className="flex items-center gap-2 px-5 py-4 border-b border-border">
          <HelpCircle className="w-5 h-5 text-primary" />
          <h2 className="font-semibold text-foreground">Input Required</h2>
          {questions.length > 1 && (
            <span className="ml-auto text-xs text-muted-foreground">
              {currentIdx + 1} / {questions.length}
            </span>
          )}
        </div>

        {/* Question */}
        <div className="px-5 py-4 space-y-4">
          {question.header && (
            <span className="inline-block px-2 py-0.5 text-xs font-medium bg-primary/10 text-primary rounded-full">
              {question.header}
            </span>
          )}
          <p className="font-medium text-foreground">{question.question}</p>

          {/* Options */}
          <div className="space-y-2">
            {question.options.map((opt, idx) => {
              const isSelected = !showOther && selectedAnswer === opt.label
              return (
                <button
                  key={idx}
                  onClick={() => selectOption(opt.label)}
                  className={`w-full text-left px-3 py-2.5 rounded-lg border transition-all ${
                    isSelected
                      ? 'border-primary bg-primary/10'
                      : 'border-border hover:border-primary/50'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-mono text-muted-foreground w-4">{idx + 1}</span>
                    <span className="text-sm font-medium text-foreground">{opt.label}</span>
                  </div>
                  {opt.description && (
                    <p className="ml-6 mt-1 text-xs text-muted-foreground">{opt.description}</p>
                  )}
                </button>
              )
            })}

            {/* Other option */}
            <button
              onClick={selectOther}
              className={`w-full text-left px-3 py-2.5 rounded-lg border transition-all ${
                showOther
                  ? 'border-primary bg-primary/10'
                  : 'border-border hover:border-primary/50'
              }`}
            >
              <div className="flex items-center gap-2">
                <span className="text-xs font-mono text-muted-foreground w-4">{question.options.length + 1}</span>
                <span className="text-sm font-medium text-foreground italic">Other</span>
              </div>
            </button>

            {showOther && (
              <input
                type="text"
                placeholder="Enter your answer..."
                value={customInput}
                onChange={(e) => {
                  setCustomInput(e.target.value)
                  setAnswers(prev => ({ ...prev, [currentIdx.toString()]: e.target.value || 'Other' }))
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault()
                    submit()
                  }
                }}
                className="w-full px-3 py-2 ml-6 border border-border rounded-lg bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                autoFocus
              />
            )}
          </div>
        </div>

        {/* Actions */}
        <div className="flex items-center justify-end px-5 py-4 border-t border-border">
          <button
            onClick={submit}
            disabled={!selectedAnswer}
            className="flex items-center gap-1.5 px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Check className="w-4 h-4" />
            {currentIdx < questions.length - 1 ? 'Next' : 'Submit'}
          </button>
        </div>
      </div>
    </div>
  )
}
