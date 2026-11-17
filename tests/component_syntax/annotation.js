import * as React from 'react';

declare const Basic: component();
<Basic />; // ok
<Basic foo={42} /> // error

declare const InlineOnly: component(
  foo: number,
  bar: string,
);

const inlineOnlyGood = <InlineOnly foo={3} bar="" />; // OK!
const inlineOnlyBad = <InlineOnly foo="" bar={3} />; // Error 2x

declare const InexactRest: component(
  foo: number,
  ...rest: {bar: string, ...}
);

const inexactGood = <InexactRest foo={3} bar="str" baz={3} />; // OK!
const inexactBad = <InexactRest foo="" bar={3} baz={3} />; // ERROR 2x for foo and bar

declare const IndexedRest: component(
  foo: number,
  ...rest: {[key: string]: number, ...}
);

const indexedGood = <IndexedRest foo={3} bar={3} />; // OK!
const indexedBad = <IndexedRest foo="" bar="str" />; // ERROR 2x for foo and bar

declare const DefaultProps: component(
  foo?: number,
);

const defaultsGood = <DefaultProps />; // OK!
const defaultsBad = <DefaultProps foo="bad" />; // ERROR

declare const OptionalRest: component(
    ...{foo?: number}
  );

const optGood = <OptionalRest />; // OK!
const optBad = <OptionalRest foo="bad" />; // ERROR

declare const Duplicate: component(x: number, ...{x: number}); // error
<Duplicate x={1} />;

declare const InlineRef: component(ref: number); // ok: ref type in componenent type can be arbitrary
<InlineRef ref={1} />;

declare const SpreadRef: component(...p: {ref: number}); // no error. spreading ref is now allowed
<SpreadRef ref={1} />; // no error. spreading ref is now allowed

declare export const Export: component(x: number) renders number; // invalid-render

declare const DefaultExport: component(x: number);
export default DefaultExport;

<Export />; // error
<DefaultExport x={"a"} />; // error
