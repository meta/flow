/**
 * @format
 * @flow
 */

42 as string | null | undefined; // Error: should only show string
42 as {} | null | undefined; // Error: should only show object
42 as [] | null | undefined; // Error: should only show array
({}) as string | null | undefined; // Error: should only show string
({}) as {p: empty} | null | undefined; // Error: should only show object
({}) as [] | null | undefined; // Error: should only show array
[] as string | null | undefined; // Error: should only show string
[] as {} | null | undefined; // Error: should only show object
[] as [empty] | null | undefined; // Error: should only show array
