declare function useCallback<T extends (...args: ReadonlyArray<empty>) => unknown>(
  callback: T,
  inputs: ?ReadonlyArray<unknown>
): T;

type ReturnValueType<T extends InputValueType> =
  T extends InputValueArrayType
    ? ReturnValueArrayType<T>
    : ReturnValueObjectType<T>;

type InputValueType = Readonly<{[key: string]: InputValueType}>;
type InputValueArrayType = ReadonlyArray<InputValueType>;

type ReturnValueObjectType<T extends InputValueType> = Readonly<{
  [key in keyof T]: ReturnValueType<T[key]>,
}>;

type ReturnValueArrayType<T extends InputValueArrayType> = ReadonlyArray<
  ReturnValueType<T[0]>,
>;

const foo: (
  id: string,
  type: InputValueType,
  key: string,
) => ReturnValueType<InputValueType> = useCallback(
  (
    id: string,
    type: InputValueType,
    key: string,
  ): ReturnValueType<InputValueType> => {
    return {
      [key]: foo(id, type[key], key),
    };
  },
  [],
);
