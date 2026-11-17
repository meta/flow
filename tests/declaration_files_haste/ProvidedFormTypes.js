import type ProvidedParams from 'ProvidedParams';

export type FormProps = Readonly<{
  onChange: (params: ProvidedParams) => void,
  params: ProvidedParams,
}>;
