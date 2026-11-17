import * as React from 'react';

type TPopover<P extends {...}> = component(...P);

declare component ContainerCP<T extends {...}>(
  f: (props: React.ComponentProps<TPopover<T>>) => void,
  z: T,
);

type O = {x: number};

ContainerCP as TPopover<{f: (props: React.ComponentProps<TPopover<O>>) => void, z: O}>; // ok

ContainerCP as TPopover<{f: (props: React.ComponentProps<TPopover<O>>) => void, z: {y: string}}>; // error: z conflicts with f

declare function g<T extends {...}>(
  f: (props: React.ComponentProps<TPopover<T>>) => void,
  z: T,
): void;
g as (f: (props: React.ComponentProps<TPopover<O>>) => void, z: O) => void; // ok
