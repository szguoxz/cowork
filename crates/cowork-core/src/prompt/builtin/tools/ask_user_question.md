# AskUserQuestion Tool Description

Use this tool when you need to ask the user questions during execution. This allows you to:
1. Gather user preferences or requirements
2. Clarify ambiguous instructions
3. Get decisions on implementation choices as you work
4. Offer choices to the user about what direction to take.

## Usage Notes

- Users will always be able to select "Other" to provide custom text input
- Use multiSelect: true to allow multiple answers to be selected for a question
- If you recommend a specific option, make that the first option in the list and add "(Recommended)" at the end of the label

## Plan Mode Note

In plan mode, use this tool to clarify requirements or choose between approaches BEFORE finalizing your plan. Do NOT use this tool to ask "Is my plan ready?" or "Should I proceed?" - use ExitPlanMode for plan approval.

## Parameters

- `questions` (required): Array of 1-4 questions to ask the user, each containing:
  - `question` (required): The complete question to ask, ending with a question mark
  - `header` (required): Very short label displayed as a chip/tag (max 12 chars)
  - `options` (required): Array of 2-4 choices, each with:
    - `label` (required): Display text for the option (1-5 words)
    - `description` (required): Explanation of what this option means
  - `multiSelect` (optional, default false): Allow selecting multiple options
