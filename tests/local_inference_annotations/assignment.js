//@flow
let x: (x: number) => number = (x) => 3;
x = (x) => 3; // No missing annot

let y = (x: number) => 3;
y = (x) => 3; // No missing annot

let [a, b]: [(x: number) => number, (y: number) => number] = [(x) => x, (y) => y]
a = (x) => x; // No missing annot
[a, b] = [(y) => y , (y) => y]; // No missing annots

var c;
c = (x) => 3; // Missing annot

var d: (x: number) => number;
d = (x) => 3; // No missing annot

let e;
e = (x) => 3; // Missing annot

let f: (x: number) => number;
f = (x) => 3; // No missing annot
