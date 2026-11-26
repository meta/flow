export type A<X extends string> = {
  data: X,
  tag: "a"
};
export type B = {
  tag: "b"
};
export type C = {
  tag: "c"
};

export type As = A<string>;
export opaque type OpaqueB = B;
export opaque type OpaqueC = C;
export opaque type OpaqueAs = As;
export opaque type OpaqueT = undefined | OpaqueB | OpaqueC | OpaqueAs;
export type OpaqueOrString = OpaqueT | string;

export opaque type Animal: string = string;
export opaque type Raccoon: Animal = string;

declare export function isAnimal(_s: string): implies _s is Animal;
declare export function isRaccoon(_s: string): implies _s is Raccoon;
