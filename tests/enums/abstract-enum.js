enum E {A, B}

// Use on concrete enum
{
  E as Enum<E>; // OK
  declare const x: Enum<E>;
  x as typeof E; // OK
}

type T = Enum<boolean>; // ERROR
E as T; // ERROR

// Abstract enum as LHS
{
  declare const x: Enum<EnumValue<string>>;
  x as Enum<>; // OK
  x as Enum<EnumValue<string | number>>; // OK

  x as Enum<EnumValue<boolean>>; // ERROR
  x as Enum<E>; // ERROR
}

// Generic usage
function f<
  TRepresentationType extends EnumRepresentationTypes,
  TEnumValue extends EnumValue<TRepresentationType>,
>(e: Enum<TEnumValue>) {
  declare const raw: TRepresentationType; // OK
  declare const val: TEnumValue;

  e.members() as Iterator<TEnumValue>; // OK
  e.cast(raw) as TEnumValue | void; // OK
  e.isValid(raw) as boolean; // OK
  e.getName(val) as string; // OK

  e.A; // ERROR
  switch (val) {} // ERROR
}
f<string, E>(E); // OK
f(E); // OK

f(true); // ERROR

// Strict Equality
{
  declare const x: Enum<>;
  E === x; // OK
  x === E; // OK

  declare const y: Enum<>;
  x === y; // OK
  y === x; // OK
}
