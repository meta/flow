async function foo(): Promise<(n: number) => void> {
  const f: (n: number) => void = (n) => {}; // ok
  return (n) => {}; // ok
}

let f: () => Promise<(n: number) => void> = async () => (n) => {}; // ok

async function bar(): Promise<(n: number) => void> {
  return Promise.resolve((n) => {}); // ok
}

declare function commit<T>(config: T['variables']): Promise<T>;

async function g(): Promise<{ variables: any }> {
  return await commit({});  // error: property 'variables' is missing (reference
                            // points to annotation above, not libdefs)
}
