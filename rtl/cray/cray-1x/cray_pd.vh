// The fields of a predecoded instruction: what cray_predecode.v makes of an
// instruction's first parcel and func_top keeps in a register beside CIP.
// Each name is the low bit of its field; the width follows in the comment.

// registers the instruction reads (cray_opnd)
localparam PD_RD_A = 0;  // 8
localparam PD_RD_S = PD_RD_A + 8;  // 8

// what kind of instruction it is
localparam PD_TWO = PD_RD_S + 8;  // 1  it has a second parcel
localparam PD_EXCH = PD_TWO + 1;  // 1  000 or 004, an exit
localparam PD_VTYPE = PD_EXCH + 1;  // 1  140 to 177
localparam PD_MTYPE = PD_VTYPE + 1;  // 1  it refers to memory
localparam PD_BTYPE = PD_MTYPE + 1;  // 1  a branch

// what it has to wait for besides its registers
localparam PD_H076 = PD_BTYPE + 1;  // 8  076: the V register it reads
localparam PD_HVM = PD_H076 + 8;  // 1  a 175 that is still building the mask
localparam PD_H003 = PD_HVM + 1;  // 1  a merge that is still using the mask
localparam PD_HFADD = PD_H003 + 1;  // 1  a vector operation in the floating point adder
localparam PD_HFMUL = PD_HFADD + 1;  // 1  one in the multiplier
localparam PD_HFRCP = PD_HFMUL + 1;  // 1  one in the reciprocal unit
localparam PD_HMODE = PD_HFRCP + 1;  // 1  every result on its way (mode instructions, 073i01)
localparam PD_TS = PD_HMODE + 1;  // 1  0034, test and set
localparam PD_H074 = PD_TS + 1;  // 1  074: a 075 that has not written its T register yet
localparam PD_H024 = PD_H074 + 1;  // 1  024: a 025 that has not written its B register yet
localparam PD_H072 = PD_H024 + 1;  // 1  072: a 0014 that has not set the clock yet
localparam PD_H073 = PD_H072 + 1;  // 1  073: a 175 whose last test is not in the mask yet

// for the S scheduler
localparam PD_S_TYPE = PD_H073 + 1;  // 1
localparam PD_S_STAGE = PD_S_TYPE + 1;  // 14  stage of the result pipeline its result enters
localparam PD_S_SRC = PD_S_STAGE + 14;  // 5  unit the result comes from
localparam PD_S_DEST = PD_S_SRC + 5;  // 8  register the result goes to, one bit a register
localparam PD_S_DNUM = PD_S_DEST + 8;  // 3  the same as a number
localparam PD_S_CMASK = PD_S_DNUM + 3;  // 8  registers that must have no result on its way
localparam PD_S_WPC = PD_S_CMASK + 8;  // 14  stage that must be empty for its result
localparam PD_S_077 = PD_S_WPC + 14;  // 1
localparam PD_S_VW = PD_S_077 + 1;  // 8  077: the V register it writes

// for the A scheduler
localparam PD_A_TYPE = PD_S_VW + 8;  // 1
localparam PD_A_STAGE = PD_A_TYPE + 1;  // 11
localparam PD_A_SRC = PD_A_STAGE + 11;  // 4
localparam PD_A_DEST = PD_A_SRC + 4;  // 8
localparam PD_A_DNUM = PD_A_DEST + 8;  // 3
localparam PD_A_CMASK = PD_A_DNUM + 3;  // 8
localparam PD_A_WPC = PD_A_CMASK + 8;  // 11
localparam PD_A_025 = PD_A_WPC + 11;  // 1
localparam PD_A_SCONF = PD_A_025 + 1;  // 8  023: the S register it reads

// for the V scheduler
localparam PD_V_I = PD_A_SCONF + 8;  // 8  V register written
localparam PD_V_J = PD_V_I + 8;  // 8  V registers read
localparam PD_V_K = PD_V_J + 8;  // 8
localparam PD_V_FU = PD_V_K + 8;  // 8  unit used

// for the branch unit
localparam PD_BR_005 = PD_V_FU + 8;  // 1
localparam PD_BR_A0 = PD_BR_005 + 1;  // 1  010 to 013
localparam PD_BR_S0 = PD_BR_A0 + 1;  // 1  014 to 017
localparam PD_BR_JMP = PD_BR_S0 + 1;  // 1  006, 007

localparam PD_W = PD_BR_JMP + 1;
