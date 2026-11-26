type Tuple = [number, string, boolean];

[1, 'foo', true] as Tuple; // OK

[ , 'foobar', true] as Tuple; // ERROR
[1, , true] as Tuple; // ERROR
[1, 'foobar', , ] as Tuple;  // ERROR
[ , , , ] as Tuple;  // ERROR


[,] as [undefined]; // OK
