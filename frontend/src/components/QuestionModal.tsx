import { useState, useCallback, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { HelpCircle, Check, ChevronDown, ChevronUp } from 'lucide-react'

interface QuestionOption {
  label: string
  description: string
}

interface UserQuestion {
  question: string
  header: string
  options: QuestionOption[]
  multi_select: boolean
}

interface QuestionModalProps {
  sessionId: string
  requestId: string
  questions: UserQuestion[]
  onAnswer?: () => void
  onCancel?: () => void
}

export default function QuestionModal({
  sessionId,
  requestId,
  questions,
  onAnswer,
  onCancel,
}: QuestionModalProps) {
  const [expanded, setExpanded] = useState(true)
  // Map of question index to selected answers
  const [answers, setAnswers] = useState<Record<string, string | string[]>>({})
  // Custom text inputs for "Other" option
  const [customInputs, setCustomInputs] = useState<Record<string, string>>({})
  // Track which questions have "Other" selected
  const [showOther, setShowOther] = useState<Record<string, boolean>>({})
  const [submitting, setSubmitting] = useState(false)

  // Initialize answers with first option for single select questions
  useEffect(() => {
    const initialAnswers: Record<string, string | string[]> = {}
    questions.forEach((q, idx) => {
      if (q.multi_select) {
        initialAnswers[idx.toString()] = []
      } else if (q.options.length > 0) {
        initialAnswers[idx.toString()] = q.options[0].label
      }
    })
    setAnswers(initialAnswers)
  }, [questions])

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Number keys 1-4 to select options for current question
      if (e.key >= '1' && e.key <= '4') {
        const optionIndex = parseInt(e.key) - 1
        // For single question, select that option
        if (questions.length === 1 && questions[0].options[optionIndex]) {
          handleOptionSelect('0', questions[0].options[optionIndex].label, questions[0].multi_select)
        }
      }

      // Enter to submit
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault()
        handleSubmit()
      }

      // Escape to cancel
      if (e.key === 'Escape') {
        e.preventDefault()
        handleCancel()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [questions, answers])

  const handleOptionSelect = useCallback(
    (questionIdx: string, optionLabel: string, multiSelect: boolean) => {
      if (optionLabel === '__other__') {
        // Toggle "Other" selection
        setShowOther((prev) => ({ ...prev, [questionIdx]: !prev[questionIdx] }))
        if (multiSelect) {
          setAnswers((prev) => {
            const current = (prev[questionIdx] as string[]) || []
            if (current.includes('__other__')) {
              return { ...prev, [questionIdx]: current.filter((v) => v !== '__other__') }
            }
            return { ...prev, [questionIdx]: [...current, '__other__'] }
          })
        } else {
          setAnswers((prev) => ({ ...prev, [questionIdx]: '__other__' }))
        }
      } else {
        setShowOther((prev) => ({ ...prev, [questionIdx]: false }))
        if (multiSelect) {
          setAnswers((prev) => {
            const current = ((prev[questionIdx] as string[]) || []).filter((v) => v !== '__other__')
            if (current.includes(optionLabel)) {
              return { ...prev, [questionIdx]: current.filter((v) => v !== optionLabel) }
            }
            return { ...prev, [questionIdx]: [...current, optionLabel] }
          })
        } else {
          setAnswers((prev) => ({ ...prev, [questionIdx]: optionLabel }))
        }
      }
    },
    []
  )

  const handleCustomInput = useCallback((questionIdx: string, value: string) => {
    setCustomInputs((prev) => ({ ...prev, [questionIdx]: value }))
  }, [])

  const handleSubmit = useCallback(async () => {
    setSubmitting(true)
    try {
      // Build final answers map
      const finalAnswers: Record<string, string> = {}
      Object.entries(answers).forEach(([idx, answer]) => {
        if (Array.isArray(answer)) {
          // Multi-select: join with commas, replace __other__ with custom text
          const resolved = answer.map((a) =>
            a === '__other__' ? customInputs[idx] || 'Other' : a
          )
          finalAnswers[idx] = resolved.join(', ')
        } else {
          // Single select: replace __other__ with custom text
          finalAnswers[idx] =
            answer === '__other__' ? customInputs[idx] || 'Other' : answer
        }
      })

      await invoke('answer_loop_question', {
        sessionId,
        requestId,
        answers: finalAnswers,
      })
      onAnswer?.()
    } catch (err) {
      console.error('Answer error:', err)
    } finally {
      setSubmitting(false)
    }
  }, [sessionId, requestId, answers, customInputs, onAnswer])

  const handleCancel = useCallback(async () => {
    try {
      await invoke('stop_loop', { sessionId })
      onCancel?.()
    } catch (err) {
      console.error('Cancel error:', err)
    }
  }, [sessionId, onCancel])

  if (questions.length === 0) {
    return null
  }

  return (
    <div
      className="
        fixed bottom-4 left-1/2 -translate-x-1/2
        w-[600px] max-w-[90vw]
        bg-white dark:bg-gray-800
        rounded-lg shadow-2xl border border-primary-500
        z-50
      "
    >
      {/* Header */}
      <div
        className="
          flex items-center justify-between px-4 py-3
          rounded-t-lg cursor-pointer
          bg-primary-50 dark:bg-primary-900/20
        "
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-2">
          <HelpCircle className="w-5 h-5 text-primary-600" />
          <span className="font-medium text-gray-900 dark:text-white">
            AI needs your input
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-gray-500">
            Press <kbd className="px-1.5 py-0.5 bg-gray-200 dark:bg-gray-600 rounded">Enter</kbd> to
            submit
          </span>
          {expanded ? <ChevronDown className="w-4 h-4" /> : <ChevronUp className="w-4 h-4" />}
        </div>
      </div>

      {/* Questions */}
      {expanded && (
        <div className="max-h-96 overflow-y-auto">
          {questions.map((question, qIdx) => (
            <div
              key={qIdx}
              className="px-4 py-4 border-t border-gray-200 dark:border-gray-700"
            >
              {/* Question header */}
              <div className="flex items-center gap-2 mb-2">
                <span className="px-2 py-0.5 text-xs bg-primary-100 dark:bg-primary-900 text-primary-700 dark:text-primary-300 rounded-full font-medium">
                  {question.header}
                </span>
                {question.multi_select && (
                  <span className="text-xs text-gray-500">(multiple selection)</span>
                )}
              </div>

              {/* Question text */}
              <p className="text-gray-900 dark:text-white font-medium mb-3">{question.question}</p>

              {/* Options */}
              <div className="space-y-2">
                {question.options.map((option, oIdx) => {
                  const isSelected = question.multi_select
                    ? ((answers[qIdx.toString()] as string[]) || []).includes(option.label)
                    : answers[qIdx.toString()] === option.label

                  return (
                    <button
                      key={oIdx}
                      onClick={() =>
                        handleOptionSelect(qIdx.toString(), option.label, question.multi_select)
                      }
                      className={`
                        w-full text-left px-3 py-2 rounded-md border transition-all
                        ${
                          isSelected
                            ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/30'
                            : 'border-gray-200 dark:border-gray-600 hover:border-primary-300 dark:hover:border-primary-500'
                        }
                      `}
                    >
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-mono text-gray-400 w-4">{oIdx + 1}</span>
                        <span className="font-medium text-gray-900 dark:text-white">
                          {option.label}
                        </span>
                      </div>
                      {option.description && (
                        <p className="ml-6 mt-1 text-sm text-gray-500 dark:text-gray-400">
                          {option.description}
                        </p>
                      )}
                    </button>
                  )
                })}

                {/* "Other" option */}
                <button
                  onClick={() =>
                    handleOptionSelect(qIdx.toString(), '__other__', question.multi_select)
                  }
                  className={`
                    w-full text-left px-3 py-2 rounded-md border transition-all
                    ${
                      showOther[qIdx.toString()]
                        ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/30'
                        : 'border-gray-200 dark:border-gray-600 hover:border-primary-300 dark:hover:border-primary-500'
                    }
                  `}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-mono text-gray-400 w-4">
                      {question.options.length + 1}
                    </span>
                    <span className="font-medium text-gray-900 dark:text-white italic">Other</span>
                  </div>
                </button>

                {/* Custom input for "Other" */}
                {showOther[qIdx.toString()] && (
                  <input
                    type="text"
                    placeholder="Enter your answer..."
                    value={customInputs[qIdx.toString()] || ''}
                    onChange={(e) => handleCustomInput(qIdx.toString(), e.target.value)}
                    className="
                      w-full px-3 py-2 ml-6 mt-1
                      border border-gray-300 dark:border-gray-600
                      rounded-md bg-white dark:bg-gray-700
                      text-gray-900 dark:text-white
                      focus:ring-2 focus:ring-primary-500 focus:border-transparent
                    "
                    autoFocus
                  />
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center justify-between px-4 py-3 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-700 rounded-b-lg">
        <button
          onClick={handleCancel}
          className="px-3 py-1.5 text-sm text-gray-600 dark:text-gray-300 hover:text-gray-800 dark:hover:text-white"
        >
          Cancel (Esc)
        </button>
        <button
          onClick={handleSubmit}
          disabled={submitting}
          className="
            flex items-center gap-1.5 px-4 py-1.5 text-sm
            bg-primary-500 hover:bg-primary-600 text-white
            rounded-md transition-colors
            disabled:opacity-50 disabled:cursor-not-allowed
          "
        >
          <Check className="w-4 h-4" />
          {submitting ? 'Submitting...' : 'Submit'}
        </button>
      </div>
    </div>
  )
}
