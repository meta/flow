// Exports values produced by `satisfies` expressions. The importer should
// see the *inferred* type, not the satisfies annotation.

export const palette = {
  red: "#ff0000",
  green: "#00ff00",
  blue: "#0000ff",
} as const satisfies {readonly [K in "red" | "green" | "blue"]: string};

export const keyedNumbers = {x: 1, y: 2, z: 3} satisfies {
  readonly [key: string]: number,
};
