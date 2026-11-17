//@flow
import * as React from 'react';

component InlineOnly(foo: number, bar: string) {
  return null;
}

const inlineOnlyGood = <InlineOnly foo={3} bar="" />; // OK!
const inlineOnlyBad = <InlineOnly foo="" bar={3} />; // Error 2x

component InexactRest(foo: number, ...rest: {bar: string, ...}) {
  return null;
}

const inexactGood = <InexactRest foo={3} bar="str" baz={3} />; // OK!
const inexactBad = <InexactRest foo="" bar={3} baz={3} />; // ERROR 2x for foo and bar

component IndexedRest(foo: number, ...rest: {[key: string]: number, ...}) {
  return null;
}

const indexedGood = <IndexedRest foo={3} bar={3} />; // OK!
const indexedBad = <IndexedRest foo="" bar="str" />; // ERROR 2x for foo and bar

component DefaultProps(foo: number = 3) {
  return null;
}

const defaultsGood = <DefaultProps />; // OK!
const defaultsBad = <DefaultProps foo="bad" />; // ERROR

component RenamedParams(
  foo as bar: number,
  bar as foo: string,
  'non-ident' as baz: string,
) {
  return <div />;
}

const renamedGood = <RenamedParams foo={3} bar="str" non-ident="str" />; // OK!
const renamedWrongProps = <RenamedParams bar={3} foo="str" baz="str" />; // ERROR 4x, bad foo type, bad bar type, no non-ident, extra baz

component HasOptionalParams(
  x?: number,
  y: number = 3,
  ...rest?: {foo: number} // ERROR, rest params can't be optional
) {
  x as number; // ERROR
  y as number; // OK!
  rest as {foo: number}; // ERROR, rest is optional in the body, as typed
  return;
}

const elNoArgs = <HasOptionalParams />;
const elOnlyX = <HasOptionalParams x={3} />;
const elOnlyY = <HasOptionalParams y={3} />;
const elBoth = <HasOptionalParams x={3} y={3} />;

component Destruct(foo: number, ...{bar}: {bar: number}) {
  return null;
}
<Destruct foo={3} bar={3} />;

component OptionalAsBinding(
  foo as bar?: number,
) {
  bar as number; // ERROR, bar is optional
  bar as number | void; // OK
  return;
}
<OptionalAsBinding />;
<OptionalAsBinding foo={3} />;
<OptionalAsBinding foo="str" />; // ERROR

component OptionalStringKeyAsBinding(
  'aria-label' as ariaLabel?: string,
) {
  ariaLabel as string; // ERROR, ariaLabel is optional
  ariaLabel as string | void; // OK
  return;
}
<OptionalStringKeyAsBinding />;
<OptionalStringKeyAsBinding aria-label="hello" />;
<OptionalStringKeyAsBinding aria-label={3} />; // ERROR

module.exports = {
  InlineOnly,
  InexactRest,
  IndexedRest,
  DefaultProps,
  RenamedParams,
  Destruct,
  OptionalAsBinding,
  OptionalStringKeyAsBinding,
};
