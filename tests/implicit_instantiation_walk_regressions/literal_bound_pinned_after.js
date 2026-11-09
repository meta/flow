import * as React from 'react';

// A literal of an argument that the bound of its type parameter needs precise through a type
// parameter pinned after it: `TRuntime` is pinned before `TOther`, whose solution, the props of
// `Modal`, needs `mode` precise.
declare component Modal(mode: 'draft' | 'published' | null);
declare function useModal<
  TOther extends {readonly mode?: unknown, ...},
  TRuntime extends {readonly mode: TOther['mode']},
  TComp extends component(...TOther),
>(c: TComp, props: TRuntime): void;

declare const isPublished: boolean;
useModal(Modal, {mode: isPublished ? 'published' : null}); // ok
useModal(Modal, {mode: isPublished ? 'published' : 'other'}); // error: 'other'
