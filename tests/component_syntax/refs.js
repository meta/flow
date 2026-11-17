component Reffed() { return null }

component Foo(ref: {current: typeof Reffed}) { return null };

component Bar(ref: ((instance: typeof Reffed | null) => void)) { return null };


component Baz(ref: React.RefSetter<typeof Reffed>) { return null };

component Qux(ref: React.RefSetter<'div'>) { return null };

Foo as component(ref: React.RefSetter<string>); // err: React.RefSetter<string> ~> {current: typeof Reffed}
Foo as component(ref: React.RefSetter<typeof Reffed>); //err

Bar as component(ref: ((instance: string | null) => void)); // err

Baz as component(ref: React.RefSetter<string>); // err

Qux as component(ref: React.RefSetter<string>); // ok


import * as React from 'react';
{ // ok
    component MyNestedInput(other: string, ref: React.RefSetter<?InputInstance>) {
        return <input id={other} ref={ref} />
    }

    component MyInput(label: string, ref: React.RefSetter<?CommonInstance>, ...otherProps: { other: string}) {
        return (
            <label>
                {label}
                <MyNestedInput {...otherProps} ref={ref} />
            </label>
            );
    }

    component Form() {
        const ref = React.useRef<?CommonInstance>(null);

        function handleClick() {
            ref.current as ?CommonInstance;
        }

        return (
            <form>
                <MyInput label="Enter your name:" ref={ref} other="whatever" />
                <button type="button" onClick={handleClick}>
                    Edit
                </button>
            </form>
        );
    }
}

{ //mismatch
    component MyNestedInput(other: string, ref: React.RefSetter<?CommonInstance>) {
        return <input id={other} ref={ref} />
    }

    component MyInput(label: string, ref: React.RefSetter<?InputInstance>, ...otherProps: { other: string}) {
        return (
            <label>
                {label}
                <MyNestedInput {...otherProps} ref={ref} />
            </label>
            );
    }

    component Form() {
        const ref = React.useRef<?CommonInstance>(null);

        function handleClick() {
            ref.current as ?CommonInstance;
        }

        return (
            <form>
                <MyInput label="Enter your name:" ref={ref} other="whatever" />
                <button type="button" onClick={handleClick}>
                    Edit
                </button>
            </form>
        );
    }
}

{
  declare component Foo(ref?: React.RefSetter<{foo: string, bar: number}>)

  declare const badRef: React.RefSetter<{foo: string, bar: boolean}>;
  declare const goodRef: React.RefSetter<{foo: string, bar: number}>;
  <Foo ref={badRef} />; // error
  <Foo ref={goodRef} />; // ok
  <Foo ref={null} />; // ok
  <Foo ref={undefined} />; // ok
  <Foo />; // ok
}

{
    component GenericRef1<T>(ref: React.RefSetter<T>) { return null };
    component GenericRef2<T>(ref: React.RefSetter<Array<Array<Array<T>>>>) { return null };

    <GenericRef1 ref={(r: string | null) => {}} />; // ok
    <GenericRef1 ref={(r: number | null) => {}} />; // ok
    <GenericRef1 ref={1} />; // error
    <GenericRef2 ref={(r: Array<Array<Array<string>>> | null) => {}} />; // ok
    <GenericRef2 ref={(r: Array<Array<Array<number>>> | null) => {}} />; // ok
    <GenericRef2 ref={(r: string | null) => {}} />; // error
}
