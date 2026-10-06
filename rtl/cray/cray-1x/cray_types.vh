// Where the result of an S register instruction comes from.  The scheduler carries
// the code down its pipeline; func_top gathers the results by it (see the S result
// bus there).
localparam SBUS_IMM = 5'b00000,  //immediate
SBUS_COMP_IMM = 5'b00001,  //complement of immediate
SBUS_S_LOG = 5'b00010,  //scalar logical
SBUS_S_SHIFT = 5'b00011,  //scalar shift, single
SBUS_S_ADD = 5'b00100,  //scalar add
SBUS_FP_ADD = 5'b00101,  //floating point add
SBUS_FP_MULT = 5'b00110,  //floating point multiply
SBUS_FP_RA = 5'b00111,  //floating point reciprocal approximation
SBUS_CONST_GEN = 5'b01000,  //transmit (Ak) or constant to Si
SBUS_V_MASK = 5'b01010,  //vector mask
SBUS_T_BUS = 5'b01011,  //transmit (Tjk) to Si
SBUS_V = 5'b01100,  //transmit an element of Vj to Si
SBUS_S_SHIFT2 = 5'b01101,  //scalar shift, double
SBUS_MEM = 5'b10100,  //memory
SBUS_NONE = 5'b10101, SBUS_INTERCPU = 5'b10110,  //072: the clock, or an X-MP shared register
SBUS_HI_SR = 5'b10111;  //073i01 - some status bits to Si

// The registers a result can be in when its clock comes (the S result bus)
localparam SSLOT_BUS = 2'd0,  //the register that gathers most units
SSLOT_LOG = 2'd1,  //the logical unit's own
SSLOT_SHIFT = 2'd2,  //the shift unit's own, single shifts
SSLOT_FADD = 2'd3;  //the floating point adder's own

// The same for the A registers
localparam ABUS_IMM = 4'b0000,  //immediate
ABUS_COMP_IMM = 4'b0001,  //complement of immediate
ABUS_SIMM = 4'b0010,  //short immediate
ABUS_S_BUS = 4'b0011,  //transmit (Sj) to Ai
ABUS_B_BUS = 4'b0100,  //transmit (Bjk) to Ai
ABUS_S_POP = 4'b0101,  //scalar population count
ABUS_A_ADD = 4'b0110,  //address add
ABUS_A_MULT = 4'b0111,  //address multiply
ABUS_S_LZ = 4'b1000,  //scalar leading zero count
ABUS_CHANNEL = 4'b1001,  //033: a 6 Mbyte channel
ABUS_MEM = 4'b1010,  //memory
ABUS_NONE = 4'b1011, ABUS_INTERCPU = 4'b1100;  //026ij7: an X-MP shared register

localparam ASLOT_BUS = 2'd0,  //the register that gathers most units
ASLOT_ADD = 2'd1,  //the address adder's own
ASLOT_POP = 2'd2,  //the population count's own
ASLOT_LZ = 2'd3;  //the leading zero count's own
