// Negated number and bigint literal initializers
declare const neg = -8;
declare const negBig = -100n;

neg as -8; // OK - literal type
negBig as -100n; // OK - literal type
neg as 8; // ERROR - -8 !== 8
