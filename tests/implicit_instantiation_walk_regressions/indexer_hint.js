declare function filterObject<TKey, TValue, TObj extends {readonly [key: TKey]: TValue, ...}>(
  object: TObj,
  callback: (value: TValue, key: TKey, obj: TObj) => boolean,
): {[key: TKey]: TValue, ...};
declare class Box<out T> {}
declare const others: {readonly a: Box<number>, readonly b: Box<string>};
declare const image: Box<boolean>;

const all = {image, ...others};

function pick(keep: Set<string>): {[string]: Box<unknown>, ...} {
  return filterObject(all, (_value, key) => keep.has(key));
}
