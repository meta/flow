// @flow

async function test() {
    const f: (s: string) => void = await ((s) => {}); // ok
}
