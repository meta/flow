import * as React from 'react';

declare component Inner<T>(onChange: (s: string) => void, value?: T);

declare component ViaComponentProps<T>(
  ...props: React.ComponentProps<typeof Inner<T>>
);

component Test() {
  return (
    <>
      <ViaComponentProps onChange={v => {}} /> {/* ok */}
    </>
  );
}
