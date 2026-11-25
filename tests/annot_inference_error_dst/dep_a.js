declare export const o: {p: number} & {q: string};
export type P = typeof o.p; // ERROR
declare export const y: number;
