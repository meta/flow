// `renders? A` and `renders* A` against `renders T`: as in subtyping, `T` also gets what renders
// nothing, and from `renders* A` an array of what it renders.
import * as React from 'react';

component Content() {
  return null;
}

declare function f<TRenders extends React.Node>(
  c: component() renders TRenders,
): TRenders;

declare const MaybeContent: component() renders? Content;
declare const StarContent: component() renders* Content;

f(MaybeContent); // was an error
f(StarContent); // was an error

f(MaybeContent) as renders? Content;
f(MaybeContent) as renders Content; // error
