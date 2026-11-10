import * as React from 'react';

component Inner(a: number, b: string) {
  return null;
}
component Other() {
  return null;
}

type Owned = {...React.PropsOf<Inner>, inner?: void};
type Provided = {inner: renders Inner};
type Props = Owned | Provided;

component Original(...props: Props) {
  const inner: renders Inner = match (props) {
    {const a, ...const rest} => <Inner a={a} {...rest} />, // ERROR
    {const inner} => inner, // OK
  };
  return inner; // OK
}

component Explicit(...props: Props) {
  const inner: renders Inner = match (props) {
    {const a, const b, ...} => <Inner a={a} b={b} />, // OK
    {const inner} => inner, // OK
  };
  return inner; // OK
}

function identity(input: renders Inner): renders Inner {
  return match (input) { const inner => inner }; // OK
}

function property(input: {inner: renders Inner}): renders Inner {
  return match (input) { {const inner} => inner }; // OK
}

function nested(input: {value: {inner: renders Inner}}): renders Inner {
  return match (input) { {value: {const inner}} => inner }; // OK
}

function tuple(input: [renders Inner]): renders Inner {
  return match (input) { [const inner] => inner }; // OK
}

function statement(input: renders Inner): renders Inner {
  match (input) {
    const inner => { return inner; } // OK
  }
}

function union(input: renders (Inner | Other)): renders (Inner | Other) {
  return match (input) { const inner => inner }; // OK
}

function optional(input: renders? Inner): renders? Inner {
  return match (input) { const inner => inner }; // OK
}

function optionalCases(input: renders? Inner): renders? Inner {
  return match (input) {
    null | undefined | false => null, // OK
    const inner => inner, // OK
  };
}

function star(input: renders* Inner): renders* Inner {
  return match (input) { const inner => inner }; // OK
}

function starCases(input: renders* Inner): renders* Inner {
  return match (input) {
    null | undefined | false => null, // OK
    const inner => inner, // OK
  };
}

declare function identityGeneric<T>(input: T): T;
declare const genericInput: renders Inner;
(match (genericInput) { const inner => identityGeneric(inner) }) as renders Inner; // OK

type GenericRender<T extends React.Node> = renders T;

function genericObjectPattern(input: GenericRender<number>): void {
  match (input) {
    {...} => {} // OK
    _ => {} // OK
  }
}

function genericLiteralPatterns(input: GenericRender<1 | 2>): void {
  match (input) { // ERROR
    1 => {} // OK
    2 => {} // OK
  }
}

function genericMixedPattern(input: GenericRender<number | typeof genericInput>): void {
  match (input) {
    {...} => {} // OK
    _ => {} // OK
  }
}

type InnerRender = renders Inner;
function namedAlias(input: InnerRender): InnerRender {
  return match (input) { const inner => inner }; // OK
}

function svg(input: renders 'svg'): renders 'svg' {
  return match (input) { const inner => inner }; // OK
}

function rejectsOther(input: renders Inner): renders Other {
  return match (input) { const inner => inner }; // ERROR
}

function rejectsNullable(input: renders? Inner): renders Inner {
  return match (input) { const inner => inner }; // ERROR
}

function rejectsArray(input: [renders Inner, renders Inner]): renders Inner {
  return match (input) { const inner => inner }; // ERROR
}

function rejectsSvg(input: renders 'svg'): renders Inner {
  return match (input) { const inner => inner }; // ERROR
}

function notExhaustive(input: renders? Inner): void {
  match (input) { // ERROR
    null | undefined | false => {} // OK
  }
}

function explicitNullable(input: ?renders Inner): renders Inner {
  return match (input) {
    null | undefined => <Inner a={1} b="" />, // OK
    const inner => inner, // OK
  };
}

component ObjectAlias(...props: Props) {
  const inner: renders Inner = match (props) {
    {const a, const b, ...} => <Inner a={a} b={b} />, // OK
    {inner: _} as provided => provided.inner, // OK
  };
  return inner; // OK
}

function rejectsRawSvg(input: 'svg'): renders 'svg' {
  return match (input) { const inner => inner }; // ERROR
}

function rejectsArrayAsMaybe(input: [renders Inner, renders Inner]): renders? Inner {
  return match (input) { const inner => inner }; // ERROR
}

declare component OptionalRenderer() renders? Inner;
function definiteElement(input: renders OptionalRenderer): void {
  match (input) {
    null | false => {} // ERROR
    _ => {} // OK
  }
}
