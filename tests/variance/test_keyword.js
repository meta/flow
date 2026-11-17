class A {}
class B extends A {};

// named properties

type RWA = {p: A}
type ROA = {readonly p: A}
type WOA = {writeonly p: A}
type RWB = {p: B}
type ROB = {readonly p: B}
type WOB = {writeonly p: B}

declare const rwA: RWA;
declare const roA: ROA;
declare const woA: WOA;

declare const rwB: RWB;
declare const roB: ROB;
declare const woB: WOB;

// dictionaries

type dRWA = {[key: string]: A};
type dROA = {readonly [key: string]: A};
type dWOA = {writeonly [key: string]: A};
type dRWB = {[key: string]: B};
type dROB = {readonly [key: string]: B};
type dWOB = {writeonly [key: string]: B};

declare const drwA: dRWA;
declare const droA: dROA;
declare const dwoA: dWOA;

declare const drwB: dRWB;
declare const droB: dROB;
declare const dwoB: dWOB;

// X ~> A
{
  // literal A
  ({p: new A} as RWA); // ok
  ({p: new A} as dRWA); // ok

  // A
  rwA as RWA; // ok
  drwA as dRWA; // ok

  // readonly A
  roA as RWA; // error
  droA as dRWA; // error

  // writeonly A
  woA as RWA; // error
  dwoA as dRWA; // error

  // literal B
  ({p: new B} as RWA); // ok
  ({p: new B} as dRWA); // ok

  // B
  rwB as RWA; // error
  drwB as dRWA; // error

  // readonly B
  roB as RWA; // error
  droB as dRWA; // error

  // writeonly B
  woB as RWA; // error
  dwoB as dRWA; // error
}

// X ~> readonly A
{
  // literal A
  ({p: new A} as ROA); // ok
  ({p: new A} as dROA); // ok

  // A
  rwA as ROA; // ok
  drwA as dROA; // ok

  // readonly A
  roA as ROA; // ok
  droA as dROA; // ok

  // writeonly A
  woA as ROA; // error
  dwoA as dROA; // error

  // literal B
  ({p: new B} as ROA); // ok
  ({p: new B} as dROA); // ok

  // B
  rwB as ROA; // ok
  drwB as dROA; // ok

  // readonly B
  roB as ROA; // ok
  droB as dROA; // ok

  // writeonly B
  woB as ROA; // error
  dwoB as dROA; // error
}

// X ~> writeonly A
{
  // literal A
  ({p: new A} as WOA); // ok
  ({p: new A} as dWOA); // ok

  // A
  rwA as WOA; // ok
  rwA as dWOA; // ok

  // readonly A
  roA as WOA; // error
  droA as dWOA; // error

  // writeonly A
  woA as WOA; // ok
  dwoA as dWOA; // ok

  // literal B
  ({p: new B} as WOA); // ok
  ({p: new B} as dWOA); // ok

  // B
  rwB as WOA; // error
  drwB as dWOA; // error

  // readonly B
  roB as WOA; // error
  droB as dWOA; // error

  // writeonly B
  woB as WOA; // error
  dwoB as dWOA; // error
}

// X ~> B
{
  // literal A
  ({p: new A} as RWB); // error
  ({p: new A} as dRWB); // error

  // A
  rwA as RWB; // error
  drwA as dRWB; // error

  // readonly A
  roA as RWB; // error
  droA as dRWB; // error

  // writeonly A
  woA as RWB; // error
  dwoA as dRWB; // error
}

// X ~> readonly B
{
  // literal A
  ({p: new A} as ROB); // error
  ({p: new A} as dROB); // error

  // A
  rwA as ROB; // error
  drwA as dROB; // error

  // readonly A
  roA as ROB; // error
  droA as dROB; // error

  // writeonly A
  woA as ROB; // error
  dwoA as dROB; // error
}

// X ~> writeonly B
{
  // literal A
  ({p: new A} as WOB); // ok
  ({p: new A} as dWOB); // ok

  // A
  rwA as WOB; // ok
  drwA as dWOB; // ok

  // readonly A
  roA as WOB; // error
  droA as dWOB; // error

  // writeonly A
  woA as WOB; // ok
  dwoA as dWOB; // ok
}

// unification
{
  // Note: these tests don't reuse the type aliases from the prelude because
  // doing so results in "naive" unification instead of rec_unify.

  [rwA] as Array<{p:A,...}> as Array<{p:A,...}>; // ok

  [roA] as Array<{readonly p:A,...}> as Array<{p:A,...}>; // error

  [woA] as Array<{writeonly p:A,...}> as Array<{p:A,...}>; // error 
  [rwA] as Array<{p:A,...}> as Array<{readonly p:A,...}>; // error

  [roA] as Array<{readonly p:A,...}> as Array<{readonly p:A,...}>; // ok

  [woA] as Array<{writeonly p:A,...}> as Array<{readonly p:A,...}>; // error 
  [rwA] as Array<{p:A,...}> as Array<{writeonly p:A,...}>; // error 
  [roA] as Array<{readonly p:A,...}> as Array<{writeonly p:A,...}>; // error 
  [woA] as Array<{writeonly p:A,...}> as Array<{writeonly p:A,...}>; // ok 
}

// summarized [incompatible-variance] error
{
  type T1 = {[k in 'f1'|'f2'|'f3'|'f4'|'f5']: number};

  declare const x1: Readonly<T1>;
  x1 as T1; // error f1...f5 incompatible

  type T2 = {...T1, f6: number};

  declare const x2: Readonly<T2>;
  x2 as T2; // error f1...f4 and others incompatible
}
