declare class C { x: number; }

var x: string = new C().x;

interface I { x: number; }

var i = new I(); // error

function testInterfaceName(o: I) {
  o.name as string; // error, name is static
  o.constructor.name as string; // ok
}


declare const y : { x? : string};

y as interface { x? : string };

({ x : 3 } as interface { x : string | number });

interface J {
  x?: number;
}

class D {
  x: number;
}

(new D) as J; // error, x is invariant

declare const o : {[key: string] : number};

o as interface {[key: string] : number};
({x : 3} as interface {[key: string] : number});

declare const o2 : interface {[key: string] : number};
o2["a"] as number;
o2["b"] as boolean; // error


declare const o3 : {[key: string] : number};

o3 as interface {[key: string] : boolean}; // error
({x : 3} as interface {[key: string] : boolean}); // error

o3 as interface {[key: number] : number}; // error
({x : 3} as interface {[key: number] : number}); // error

declare const o4 : {[key: "asdf"] : number};

o4 as interface {[key: string] : number}; // error

declare const o5 : {readonly [key: "asdf"] : number};

o5 as interface {readonly [key: string] : number}; // OK

declare const o6 : {[key: string] : number};

o6 as interface {[key: "asdf"] : number}; // error
