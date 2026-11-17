import * as React from 'react';

// A spread of a type parameter with props written inline beside it, against an object: once the
// type parameter has a lower bound from the other arguments, the spread meets the object, so the
// inline props meet the object's own, as when the flow-based solve flows the spread into it. `R`
// is what the modal's `onComplete` takes, `string`, not its default `null`.
type ModalProps<T, R = null> = Readonly<{
  ...T,
  onHide: () => unknown,
  onComplete: (v: ?R) => unknown,
}>;
declare function showModal<T, R = null>(
  Component: React.ComponentType<ModalProps<T, R>>,
  props: T,
): Promise<?R>;

declare component Modal(
  item: ?{id: string},
  onHide: () => unknown,
  onComplete: (id: string) => unknown,
);
declare const item: {id: string};

async function f() {
  const result = await showModal(Modal, {item}); // was an error
  result as ?number; // error
}

// Also when the props are written before the spread.
type HeadModalProps<T, R = null> = Readonly<{
  onHide: () => unknown,
  onComplete: (v: ?R) => unknown,
  ...T,
}>;
declare function showHeadModal<T, R = null>(
  Component: React.ComponentType<HeadModalProps<T, R>>,
  props: T,
): Promise<?R>;

showHeadModal(Modal, {item}); // was an error
