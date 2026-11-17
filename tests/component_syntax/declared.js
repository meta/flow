import * as React from 'react';

declare component Basic();
<Basic />; // ok
<Basic foo={42} /> // error

declare component InlineOnly(
  foo: number,
  bar: string,
);

const inlineOnlyGood = <InlineOnly foo={3} bar="" />; // OK!
const inlineOnlyBad = <InlineOnly foo="" bar={3} />; // Error 2x

declare component InexactRest(
  foo: number,
  ...rest: {bar: string, ...}
);

const inexactGood = <InexactRest foo={3} bar="str" baz={3} />; // OK!
const inexactBad = <InexactRest foo="" bar={3} baz={3} />; // ERROR 2x for foo and bar

declare component IndexedRest(
  foo: number,
  ...rest: {[key: string]: number, ...}
);

const indexedGood = <IndexedRest foo={3} bar={3} />; // OK!
const indexedBad = <IndexedRest foo="" bar="str" />; // ERROR 2x for foo and bar

declare component DefaultProps(
  foo?: number,
);

const defaultsGood = <DefaultProps />; // OK!
const defaultsBad = <DefaultProps foo="bad" />; // ERROR

declare component OptionalRest(
    ...rest: {foo?: number}
  );

const optGood = <OptionalRest />; // OK!
const optBad = <OptionalRest foo="bad" />; // ERROR

declare component lowercase(); // error

declare component Duplicate(x: number, ...rest: {x: number}); // error
<Duplicate x={1} />;

declare component InlineRef(ref: number); // error
<InlineRef ref={1} />;

declare component SpreadRef(...p: {ref: number}); // ok: allowed in full ref-as-prop support
<SpreadRef ref={1} />; // ok

declare export component Export(x: number) renders number;

declare export default component DefaultExport(x: number);

<Export />; // error
<DefaultExport x={"a"} />; // error

declare component OptionalStringKeyAsBinding(
  'aria-label' as ariaLabel?: string,
);

<OptionalStringKeyAsBinding />; // OK
<OptionalStringKeyAsBinding aria-label="hello" />; // OK
<OptionalStringKeyAsBinding aria-label={3} />; // ERROR

declare component Poly<T>();

type PolyT = component<T>();
