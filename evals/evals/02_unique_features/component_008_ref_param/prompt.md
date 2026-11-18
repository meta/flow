Write Flow React components in `main.js` for a form system where parent components can imperatively focus inputs.

`TextInput` takes props `value: string`, `onChange: (value: string) => void`, `placeholder: string`, and a ref that gives the parent access to the underlying `HTMLInputElement`. It renders an `<input>` element with the given props and attaches the ref to the DOM element. When the input changes, extract the target's value and pass it to `onChange`.

`TextArea` takes props `value: string`, `onChange: (value: string) => void`, `rows: number`, and a ref for `HTMLTextAreaElement`. It renders a `<textarea>` with the given props and ref attached. When the textarea changes, extract the target's value and pass it to `onChange`.

`FocusableForm` renders a `<form>` containing:
- A `TextInput` with placeholder "Name" — create a ref, and when a "Focus Name" button is clicked, focus it
- A `TextArea` with 4 rows — create a ref, and when a "Focus Bio" button is clicked, focus it
- Both buttons should be rendered after the inputs

Use `useRef` to create the refs. Initialize them to `null`.

The code must pass `flow check` with zero errors.
