type NoProps = Values<{}>;
const noReadProps = { set a(value: number) { /* noop */ } };
type NoReadProps = Values<typeof noReadProps>;
123 as NoProps; // Error: There are no props.
345 as NoReadProps; // Error: There are no props which can be read.

type OneProp = Values<{ a: string }>;
'yo' as OneProp; // OK: There is a property with the type of string.
123 as OneProp; // Error: There is no property with the type of number.
true as OneProp; // Error: There is no property with the type of boolean.
null as any as OneProp as string | number; // OK: The values are a subset.
null as any as OneProp as number; // Error: There is no string in the final
                                  // union.

type ManyProps = Values<{ a: string, b: string, c: number }>;
'yo' as ManyProps; // OK: There is a property with the type of string.
123 as ManyProps; // OK: There is a property with the type of number.
true as ManyProps; // Error: There is no property with the type of boolean.
null as any as ManyProps as string | number | boolean; // OK: The values are a
                                                       // subset.
null as any as ManyProps as string | boolean; // Error: There is no number in
                                              // the final union.

type DictProps = Values<{ a: boolean, [key: string]: number }>;
'yo' as DictProps; // Error: There is no property with the type of string.
123 as DictProps; // OK: There is a dictionary value with the type of number.
true as DictProps; // OK: There is a property with the type of boolean.
null as any as DictProps as string | number | boolean; // OK: The values are a
                                                       // subset.
null as any as DictProps as string | boolean; // Error: There is no number in
                                              // the final union.

interface CallableProp { a: string; b: number; (): boolean }
'yo' as Values<CallableProp>; // OK: There is a property with the type of
                               // string.
123 as Values<CallableProp>; // OK: There is a property with the type of
                              // number.
true as Values<CallableProp>; // Error: There is no property with the type of
                               // boolean even though the interface is callable
                               // and may return a boolean.
(() => true) as Values<CallableProp>; // Error: There is no property with a
                                       // function of this signature even though
                                       // the interface is callable with this
                                       // signature.

const Suite: {
  DIAMONDS: 'Diamonds',
  CLUBS: 'Clubs',
  HEARTS: 'Hearts',
  SPADES: 'Spades',
} = {
  DIAMONDS: 'Diamonds',
  CLUBS: 'Clubs',
  HEARTS: 'Hearts',
  SPADES: 'Spades',
};

type SuiteEnum = Values<typeof Suite>;

const DIAMONDS: 'Diamonds' = 'Diamonds';

function magicTrick(suite: SuiteEnum) {
  // ...
}

'Diamonds' as SuiteEnum; // OK: 'Diamonds' is a valid value.
DIAMONDS as SuiteEnum; // OK: The value of `DIAMONDS` is the valid value
                       // 'Diamonds'.
'DIAMONDS' as SuiteEnum; // Error: 'DIAMONDS' is a key, but not a value.
'Magic' as SuiteEnum; // Error: 'Magic' is not a value.
'Diamonds' as string as SuiteEnum; // Error: the `string` type is to general and
                                   // not a value.

magicTrick('Diamonds'); // OK: 'Diamonds' is a valid value.
magicTrick(DIAMONDS); // OK: The value of `DIAMONDS` is the valid value
                      // 'Diamonds'.
magicTrick('DIAMONDS'); // Error: 'DIAMONDS' is a key, but not a value.
magicTrick('Magic'); // Error: 'Magic' is not a value.
magicTrick('Diamonds' as string); // Error: the `string` type is to general and
                                  // not a value.

// same as Suite above, but uses Object.freeze instead of needing an
// annotation.
const FrozenSuite = Object.freeze({
  DIAMONDS: 'Diamonds',
  CLUBS: 'Clubs',
  HEARTS: 'Hearts',
  SPADES: 'Spades',
});
type FrozenSuiteEnum = Values<typeof FrozenSuite>
'Diamonds' as FrozenSuiteEnum; // ok
DIAMONDS as FrozenSuiteEnum; // ok
'DIAMONDS' as FrozenSuiteEnum; // Error: 'DIAMONDS' is a key, but not a value.
'Magic' as FrozenSuiteEnum; // Error: 'Magic' is not a value.
'Diamonds' as string as FrozenSuiteEnum; // Error: `string` is too general


const Numbers = Object.freeze({
  foo: -1,
  bar: 2,
});
type NumbersEnum = Values<typeof Numbers>
-1 as NumbersEnum; // ok
2 as NumbersEnum; // ok
1 as NumbersEnum; // error, NumbersEnum = -1 | 2

interface IfaceWithDict {
  [key: string]: 1;
}
interface IfaceWithWriteOnlyDict {
  writeonly [key: string]: 1;
}
interface IfaceWithDictAndProps {
  [key: string]: 1;
  z: 2;
  method(): void;
}

1 as Values<IfaceWithDict>; // OK
's' as Values<IfaceWithDict>; // ERROR
1 as Values<IfaceWithWriteOnlyDict>; // ERROR
1 as Values<IfaceWithDictAndProps>; // OK
2 as Values<IfaceWithDictAndProps>; // OK
's' as Values<IfaceWithDictAndProps>; // ERROR
(() => {}) as Values<IfaceWithDictAndProps>; // ERROR
