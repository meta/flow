declare const explicitThis: (this: {value: number}, x: string) => void;
  explicitThis;
//^

declare const mixedThis: (this: mixed, x: string) => void;
  mixedThis;
//^

declare const anyThis: (this: any, x: string) => void;
  anyThis;
//^
