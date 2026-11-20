declare function cleanup(): void;

function deeplyNested(c: number): void {
  L1_0: L1_1: L1_2: try {
    if (c === 1) break L1_0;
    if (c === 2) break L1_1;
    if (c === 3) break L1_2;
  } finally {
    L2_0: L2_1: L2_2: try {
      if (c === 4) break L2_0;
      if (c === 5) break L2_1;
      if (c === 6) break L2_2;
    } finally {
      L3_0: L3_1: L3_2: try {
        if (c === 7) break L3_0;
        if (c === 8) break L3_1;
        if (c === 9) break L3_2;
      } finally {
        L4_0: L4_1: L4_2: try {
          if (c === 10) break L4_0;
          if (c === 11) break L4_1;
          if (c === 12) break L4_2;
        } finally {
          L5_0: L5_1: L5_2: try {
            if (c === 13) break L5_0;
            if (c === 14) break L5_1;
            if (c === 15) break L5_2;
          } finally {
            L6_0: L6_1: L6_2: try {
              if (c === 16) break L6_0;
              if (c === 17) break L6_1;
              if (c === 18) break L6_2;
            } finally {
              cleanup();
            }
          }
        }
      }
    }
  }
}
