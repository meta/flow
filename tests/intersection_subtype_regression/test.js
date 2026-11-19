type NonSemanticColor =
  | 'gray'
  | 'blue';

type IconColor =
  | 'active'
  | 'button'
  | NonSemanticColor;

type MediaColor = 'default' | NonSemanticColor;

{
  declare const x: IconColor & MediaColor;
  x as MediaColor; // OK: (IconColor & MediaColor) <: MediaColor
}
{
  declare const x: IconColor & MediaColor;
  x as IconColor; // OK: (IconColor & MediaColor) <: IconColor
}
{
  declare const x: IconColor & MediaColor;
  x as MediaColor; // OK
  x as IconColor;  // OK
}
function typeParamBoundedByIntersection<T extends IconColor & {...}>(x: T): IconColor {
  return x; // OK: T <: (IconColor & {...}) <: IconColor
}
function typeParamBoundedByIntersectionInUnion<T extends {a: 1} & {b: 2}>(x: T): T | null {
  return x; // OK
}
