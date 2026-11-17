
import * as React from 'react';

component Foo(a: number, ...props: {a: boolean}) { return <div /> } // Error
<Foo a={false} /> // Error: first param wins, so a must be number

type BarT = {a: string} | {b: string}
component Bar(a: number, ...props: BarT) { return <div /> }; // Error
<Bar a={false} /> // Error: first param wins, so a must be number

component Baz(a: number, ...props: {[key: string]: boolean}) { return <div /> }; // Ok
<Baz a={false} />; // Error
<Baz a={42} /> // Ok

component Qux(a: number, ...props: {[key: string]: boolean, a: number}) { return <div /> }; // Error
<Qux a={false} /> // Error: first param wins, so a must be number

type ROMSVType = {a: boolean}
component RunningOutOfMetasyntaxticVariables(a: number, ...props: {...ROMSVType}) { return <div /> }; // Error
<RunningOutOfMetasyntaxticVariables a={false} /> // Error: first param wins, so a must be number

export component NoRef(ref: string) { return <div /> }; // Error
<NoRef /> // error again due to bad ref

component NoRefInSpread(...props: {ref: string}) { return <div /> }; // Error
<NoRefInSpread />

component MyPropsAreBadInSoManyDifferentWays(foo: string, bar: string, ...props: {foo: number, bar: number, ref: any}) { return <div /> }; // Error
