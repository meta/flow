type Box = {readonly type: "GOOD", readonly value: string} | {readonly type: "BAD", readonly value: string};
type Result = {readonly type: "PENDING"} | {readonly type: "DONE", value: number};
function coerce(t: string): number {
  let box: Box = { type: "GOOD", value: t };
  function* broken(): IteratorObject<Result> {
    if (box.type === "GOOD") {
      yield { type: "PENDING" };
      if (box.type !== "GOOD") {
        yield { type: "DONE", value: (box as empty).value };
      }
    }
  }
  const it = broken();
  it.next(); // consume
  box = { type: "BAD", value: t };
  const result = it.next();
  if (result.done || result.value.type !== "DONE")
    throw new Error("Unreachable.");
  return result.value.value;
}
