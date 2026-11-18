Write Flow React components in `main.js` for a styled input system.

`BaseInput` takes props `value: string`, `onChange: (value: string) => void`, and `placeholder: string`. It renders an `<input>` element with these attributes. When the input fires its change event, extract the target's value and pass it to `onChange`.

`SearchInput` adds search-specific behavior on top of `BaseInput`. It accepts all of `BaseInput`'s props (without re-listing them individually) plus an additional `onSearch: (query: string) => void` prop. It renders a `<div>` containing `BaseInput` (forwarding the base props) and a `<button>` labeled "Search". When the button is clicked, call `onSearch` with the current `value`.

`LabeledInput` wraps `BaseInput` with a label. It accepts all of `BaseInput`'s props (without re-listing them individually) plus a `label: string` prop. It renders a `<label>` element containing the label text followed by `BaseInput` (forwarding the base props).

The code must pass `flow check` with zero errors.
