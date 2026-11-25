type Foo = { x : string, y : string };
let a : Foo = {x : "", y : ""};
let b1 : Partial<Foo> = {x : ""};
let b2 : Partial<Foo> = {y : ""};
let b3 : Partial<Foo> = {x : undefined};
let b4 : Partial<Foo> = {y : undefined};
let b5 : Partial<Foo> = {x : undefined, y : undefined};
let b6 : Partial<Foo> = {x : "", y : ""};
let b7 : Partial<Foo> = {x : null}; // error
let b8 : Partial<Foo> = {x : 3}; // error
let b9 : Partial<Foo> = {z : ""}; // error


type Void = Partial<void>;
let v : Void = undefined;
let v2 : Void = null; // error

type Null = Partial<null>;
let n : Null = undefined; // error
let n2 : Null = null;

type PFoo = Partial<?Foo>;
let p : PFoo = null;
let p2 : PFoo = undefined;
let p3 : PFoo = a; // error
let p4 : PFoo = b1;
let p5 : PFoo = {x : ""};
let p6 : PFoo = 3; // error

type PMaybeFooAndBar = Partial<?Foo & {z : string}>;
let q : PMaybeFooAndBar = {x : "", z : ""};
let q2 : PMaybeFooAndBar = {x : 3}; // error

type Err = Partial<number> // error
3 as Err;

interface IFace {
    x : string,
    y : string,
}

let c : IFace = {x : "", y : ""};
let d1 : Partial<IFace> = {x : ""};
let d2 : Partial<IFace> = {y : ""};
let d3 : Partial<IFace> = {x : undefined};
let d4 : Partial<IFace> = {y : undefined};
let d5 : Partial<IFace> = {x : undefined, y : undefined};
let d6 : Partial<IFace> = {x : "", y : ""};
let d7 : Partial<IFace> = {x : null}; // error
let d8 : Partial<IFace> = {x : 3}; // error
let d9 : Partial<IFace> = {z : 3}; // ok, interfaces are inexact

interface JFace extends IFace {
    z : string
}

let e : JFace = {x : "", y : "", z : ""};
let f1 : Partial<JFace> = {x : ""};
let f2 : Partial<JFace> = {y : ""};
let f3 : Partial<JFace> = {z : ""};
let f4 : Partial<JFace> = {x : undefined};
let f5 : Partial<JFace> = {y : undefined};
let f6 : Partial<JFace> = {z : undefined};
let f7 : Partial<JFace> = {x : undefined, y : undefined, z : ""};
let f8 : Partial<JFace> = {x : "", y : "", z : undefined};
let f9 : Partial<JFace> = {x : null}; // error
let f10 : Partial<JFace> = {z : 3}; // error
let f11 : Partial<JFace> = {x : 3}; // error
