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

// The lanes of results to the S registers (res_lanes.v): one for every unit, as
// long as the unit takes.  The first five deliver in the clock the result is
// due; the others have it a clock before and write it then (res_regfile.v;
// func_top says which units share a way into the registers).
// The lane of memory has no delay line (0): the memory unit says when a word
// it has read is at the registers.
localparam SL_LOG = 0,  //the logical unit, 1 clock
SL_NOW = 1,  //what is there when the instruction issues: an immediate value, the vector mask, the clock, a shared, status or T register
SL_SHIFT = 2,  //the shift unit, single shifts, 2 clocks
SL_FADD = 3,  //the floating point adder, 6
SL_MEM = 4,  //a word from memory
SL_CONST = 5,  //(Ak) or a constant, 2
SL_SH2 = 6,  //the shift unit, double shifts, 3
SL_ADD = 7,  //the adder, 3
SL_VEL = 8,  //an element of a V register, 4
SL_FMUL = 9,  //the floating point multiplier, 7
SL_FRA = 10,  //the reciprocal, 14
SL_N = 11;
localparam [4*SL_N-1:0] SL_DELAY = {4'd14, 4'd7, 4'd4, 4'd3, 4'd3, 4'd2, 4'd0, 4'd6, 4'd2, 4'd1, 4'd1};

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

// The lanes of results to the A registers, the first five delivering in the
// clock the result is due
localparam AL_NOW = 0,  //what is there when the instruction issues: an immediate value, (Sj), a B or shared register
AL_ADD = 1,  //the address adder, 2 clocks
AL_LZ = 2,  //the leading zero count, 3
AL_POP = 3,  //the population count, 4
AL_MEM = 4,  //a word from memory
AL_MUL = 5,  //the address multiplier, 4
AL_CH = 6,  //033: a channel, 4
AL_N = 7;
localparam [4*AL_N-1:0] AL_DELAY = {4'd4, 4'd4, 4'd0, 4'd4, 4'd3, 4'd2, 4'd1};
