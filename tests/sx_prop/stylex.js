// @flow
export type CompiledStyles = {readonly [key: string]: string};
export type StyleXArray<out T> = T | ReadonlyArray<StyleXArray<T>>;

declare function props(
  ...styles: ReadonlyArray<StyleXArray<?boolean | CompiledStyles>>
): {className: string, style: {[key: string]: string | number}};

declare function create<S extends {readonly [key: string]: {readonly [key: string]: unknown}}>(styles: S): S;

declare export default {
  props: typeof props,
  create: typeof create,
};
