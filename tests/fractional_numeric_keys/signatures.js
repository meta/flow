// Exported annotations and inferred object signatures retain fractional names.
export type ExportedFractional = {0.5: string, '1.5': number}; // OK
export const exported = {0.5: 'half', 1.5: 15, '-1.5': true}; // OK
export function roundtrip(value: ExportedFractional): ExportedFractional {
  return value; // OK
}

// A numeric and quoted access in an exported signature resolve identically.
export type NumericValue = ExportedFractional[0.5]; // OK
export type QuotedValue = ExportedFractional['0.5']; // OK
