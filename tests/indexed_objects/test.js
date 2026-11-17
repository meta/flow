//@flow
const x: { [key: string]: number } = {foo: 3}; // Ignores exact bars

type Y = { ...{[key: string]: number}, foo: number};
const y: Y = {foo: 3}; // No error

type Z = $Exact<{[key: string]: number}>;
const z: Z = {foo: 3}; // No error

type Props = {
  [key: StringPrefix<'data-'>]: string | void,
}

const error1: Props = {foo: 3}; // One error

type ObjWithProto = {
  __proto__: {foo: number},
  [key: StringPrefix<'bar'>]: string,
};

declare const objWithProto: ObjWithProto;
objWithProto.foo as number; // OK
