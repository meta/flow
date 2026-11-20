// The declaration of `x` has level error and every error below references it,
// but the reported level only depends on the level at the error's primary location.

enum E {A, B}

//flowlint-next-line require-explicit-enum-switch-cases:error
declare const x: E;

switch (x) { default: } //suppressed by default

//flowlint require-explicit-enum-switch-cases:warn
switch (x) { default: } //appears as a warning

//flowlint require-explicit-enum-switch-cases:error
switch (x) { default: } //appears as an error
