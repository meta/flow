interface WithDict {
  [key: 'x' | 'y']: number;
}

interface WithProps {
  a: number;
  method(): void;
}

interface WithBoth {
  [key: 'x' | 'y']: number;
  a: number;
  method(): void; // non-own and ignored
}

declare const xy: 'x' | 'y';

xy as keyof WithDict; // OK
'xxx' as keyof WithDict; // ERROR

'a' as keyof WithProps; // OK
'xxx' as keyof WithProps; // ERROR

'a' as keyof WithBoth; // OK
xy as keyof WithBoth; // OK
'xxx' as keyof WithBoth; // ERROR
