declare class LibOther {
  o: number;
}
declare var LibCycleBase: Class<LibCycle> & Class<LibOther>;
declare class LibCycle extends LibCycleBase {}
