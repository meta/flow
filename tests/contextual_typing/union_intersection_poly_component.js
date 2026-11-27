import * as React from 'react';

// A generic member of a union or intersection of components is instantiated from the props, as a
// generic component is.
declare component Poly<T>(value: T, onChange: (v: T) => void);
declare component Mono(value: string, onChange: (v: string) => void);

declare const U: typeof Poly | typeof Mono;
<U value="a" onChange={v => v as string} />; // ok

declare const I: typeof Poly & typeof Mono;
<I value="a" onChange={v => v as string} />; // ok

// A member that refers back to the intersection is not instantiated again.
declare component C(f: (v: string) => void);
type R<X> = typeof C & R<X>;
declare const RI: R<number>;
<RI f={v => {}} />;
type RU<X> = typeof C | (typeof C & RU<X>);
declare const RUI: RU<number>;
<RUI f={v => {}} />;
type A<X> = typeof C & B<X>;
type B<X> = typeof C | A<X>;
declare const AI: A<number>;
<AI f={v => {}} />;

type Id<X> = X;
declare const IdU: Id<typeof Poly | typeof Mono>;
<IdU value="a" onChange={v => v as string} />; // ok
type Compound<X> = typeof Poly & {Item: Compound<X>};
declare const CC: Compound<number>;
<CC value="a" onChange={v => v as string} />; // ok
