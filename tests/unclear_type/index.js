// @flow strict

const a: any = 5;

const b: number = 999;

const c: Object = {a: 5};

const d: Function = () => 0;

const e: () => any = () => 0;

const f: (o: Object) => Function = ({}) => function () {};

type f2 = <T extends Object>(x: T) => T;

// malformed types should not also be unclear
null as Object<unknown>;
null as Function<unknown>;
