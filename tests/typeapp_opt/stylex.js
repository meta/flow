// minimized from https://github.com/facebook/stylex/blob/10501b354a02776dcb55229ed1bae6aef54875c0/packages/stylex/src/StyleXTypes.js#L69-L94
import type {StyleXClassNameFor, StyleXVar} from './opaque';

export type MapNamespaces<out S extends {readonly [key: string]: unknown}> =
  Readonly<{
    [Key in keyof S]: S[Key] extends ((
      ...args: infer Args
    ) => infer Obj extends {readonly [key: string]: unknown})
      ? (...args: Args) => Readonly<[MapNamespace<Obj>, InlineStyles]>
      : S[Key] extends infer CSS extends {readonly [key: string]: unknown}
        ? MapNamespace<CSS>
        : empty,
  }>;

export type MapNamespace<out CSS extends {readonly [key: string]: unknown}> = {
  readonly [Key in keyof CSS]: StyleXClassNameFor<Key, ComplexStyleValueType<CSS[Key]>>,
};

export type InlineStyles = Readonly<{
  // $$css?: void,
  $$inline: true,
  [key: string]: string | number,
}>;

type ComplexStyleValueType<out T> =
  T extends StyleXVar<infer U>
    ? U
    : T extends string | number | null
      ? T
      : T extends ReadonlyArray<infer U>
        ? ComplexStyleValueType<U>
        : T extends { readonly default: infer A, readonly [key: string]: infer B }
          ? ComplexStyleValueType<A> | ComplexStyleValueType<B>
          : Readonly<T>;


declare const x: MapNamespaces<{readonly item: {readonly hover: {readonly backgroundColor: '#f9f2ec'}}}>;
x as MapNamespaces<{item: {hover: {backgroundColor: '#f9f2ec'}}}>; // okay
