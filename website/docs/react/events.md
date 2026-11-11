---
title: Event Handling
slug: /react/events
description: "How to type React event handlers and synthetic events in Flow."
---

The [React docs for handling events](https://react.dev/learn/responding-to-events) show how an event handler can be attached to
a React element. To type these event handlers you may use React's event types,
such as `React.MouseEvent<T>`, like this:

```js flow-check
import {useState} from 'react';
import * as React from 'react';

component MyComponent() {
  const [state, setState] = useState({count: 0});

  const handleClick = (event: React.MouseEvent<HTMLButtonElement>) => {
    // To access your button instance use `event.currentTarget`.
    event.currentTarget as HTMLButtonElement;

    setState(prevState => ({
      count: prevState.count + 1,
    }));
  };

  return (
    <div>
      <p>Count: {state.count}</p>
      <button onClick={handleClick}>Increment</button>
    </div>
  );
}
```

There are also other event types like `React.KeyboardEvent<T>`,
`React.ChangeEvent<T>`, or `React.TouchEvent<T>`, all based on
`React.SyntheticEvent<T>`. These event types typically take a single type
argument: the type of the HTML element the event handler was placed on.

If you don't want to add the type of your element instance you can also use
`React.SyntheticEvent` with *no* type arguments.

> **Note:** To get the element instance, like `HTMLButtonElement` in the example
> above, it is a common mistake to use `event.target` instead of
> `event.currentTarget`. The reason you want to use `event.currentTarget` is
> that `event.target` may be the wrong element due to [event propagation][].

[event propagation]: https://developer.mozilla.org/en-US/docs/Web/API/Document_Object_Model/Examples#example_5_event_propagation

> **Note:** React uses its own event system so it is important to use the
> React event types instead of the DOM types such as `Event`,
> `KeyboardEvent`, and `MouseEvent`.

The event types that React provides and the DOM events they are related to are:

- `React.SyntheticEvent<T>` for [Event](https://developer.mozilla.org/en-US/docs/Web/API/Event)
- `React.AnimationEvent<T>` for [AnimationEvent](https://developer.mozilla.org/en-US/docs/Web/API/AnimationEvent)
- `React.CompositionEvent<T>` for [CompositionEvent](https://developer.mozilla.org/en-US/docs/Web/API/CompositionEvent)
- `React.InputEvent<T>` for [InputEvent](https://developer.mozilla.org/en-US/docs/Web/API/InputEvent)
- `React.UIEvent<T>` for [UIEvent](https://developer.mozilla.org/en-US/docs/Web/API/UIEvent)
- `React.FocusEvent<T>` for [FocusEvent](https://developer.mozilla.org/en-US/docs/Web/API/FocusEvent)
- `React.KeyboardEvent<T>` for [KeyboardEvent](https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent)
- `React.MouseEvent<T>` for [MouseEvent](https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent)
- `React.PointerEvent<T>` for [PointerEvent](https://developer.mozilla.org/en-US/docs/Web/API/PointerEvent)
- `React.DragEvent<T>` for [DragEvent](https://developer.mozilla.org/en-US/docs/Web/API/DragEvent)
- `React.WheelEvent<T>` for [WheelEvent](https://developer.mozilla.org/en-US/docs/Web/API/WheelEvent)
- `React.TouchEvent<T>` for [TouchEvent](https://developer.mozilla.org/en-US/docs/Web/API/TouchEvent)
- `React.ClipboardEvent<T>` for [ClipboardEvent](https://developer.mozilla.org/en-US/docs/Web/API/ClipboardEvent)
- `React.TransitionEvent<T>` for [TransitionEvent](https://developer.mozilla.org/en-US/docs/Web/API/TransitionEvent)

`onChange` handlers receive a `React.ChangeEvent<T>`.

## See Also {#toc-see-also}

- [Functions](../types/functions.md) — event handlers are typed as function types
- [Generics](../types/generics.md) — synthetic event types are generic over the DOM element type
