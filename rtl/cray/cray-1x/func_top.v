//////////////////////////////////////////////////////////////////
//        Cray Functional Unit Top-level                        //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This is the "functional unit" top-level block. It instantiates all
//of the register files (A, B, S, T and V), as well as all 13 functional
//units for computing (logical, math, etc.), and all of the logic to handle
//the program counter, branching, etc.
//
module func_top (
	clk,
	rst,
	i_cpu_num,
	i_nip_nxt,
	i_word_nxt,
	i_nip_vld,
	o_p_addr,
	o_clear_ibufs,
	o_mem_ce,
	o_mem_addr,
	i_data_from_mem,
	o_data_to_mem,
	o_mem_wr_en,
	i_mem_ack,
	//DMA interface
	o_dma_instr,
	o_dma_mon_mode,
	o_dma_instr_vld,
	o_dma_ak,
	o_dma_aj,
	i_dma_ai,
	i_dma_int,
	//Inter-cpu interface
	o_cln,
	o_intercpu_instr,
	o_intercpu_monmode,
	o_intercpu_instr_vld,
	i_intercpu_issue,
	o_intercpu_sj,
	o_intercpu_si,
	o_intercpu_ai,
	i_intercpu_si,
	i_intercpu_ai,
	//Debug interface
	o_debug,
	i_debug_full,
	i_single_step,
	i_console_int,
	i_ibuf_busy,
	o_ibuf_hold
);



	// XMP = 0: CRAY-1 behaviour (manual 2240004 rev C).  XMP = 1: the X-MP additions of
	// the source this came from, kept compilable but not verified.
	parameter XMP = 0;

	`include "cray_types.vh"
	//system signals
	input wire clk;
	input wire rst;
	input wire [1:0] i_cpu_num;
	input wire [15:0] i_nip_nxt;
	input wire [63:0] i_word_nxt;
	input wire i_nip_vld;
	output wire [23:0] o_p_addr;
	output wire o_clear_ibufs;
	//memory interface
	output wire o_mem_ce;
	output wire [21:0] o_mem_addr;
	input wire [63:0] i_data_from_mem;
	output reg [63:0] o_data_to_mem;
	output wire o_mem_wr_en;
	input wire i_mem_ack;

	//I/O interface
	output wire [15:0] o_dma_instr;
	output wire o_dma_mon_mode;
	output wire o_dma_instr_vld;
	output wire [23:0] o_dma_ak;
	output wire [23:0] o_dma_aj;
	input wire [23:0] i_dma_ai;
	input wire i_dma_int;

	//inter-CPU communications
	output wire [2:0] o_cln;
	output wire [15:0] o_intercpu_instr;
	output wire o_intercpu_monmode;
	output wire o_intercpu_instr_vld;
	input wire i_intercpu_issue;
	output wire [63:0] o_intercpu_sj;
	output wire [63:0] o_intercpu_si;
	output wire [23:0] o_intercpu_ai;
	input wire [63:0] i_intercpu_si;
	input wire [23:0] i_intercpu_ai;

	output wire [31:0] o_debug;
	input wire i_debug_full;
	input wire i_single_step;
	input wire i_console_int;  // CRAY-1: request the console interrupt (flag bit 31)
	input wire i_ibuf_busy;  // an instruction buffer fill is in progress
	output wire o_ibuf_hold;  // do not start a new fill (exchange in progress)
	//Functional unit outputs
	wire [23:0] a_poplz_out;  //24-bit output of scalar population/leading zero count
	wire [23:0] a_mul_out;  //24-bit output of address multiply unit
	wire [23:0] a_add_out;  //24-bit output of address adder
	wire [23:0] a_imm_out;  //24-bit output of immediate values
	wire [63:0] s_add_out;  //64-bit output of scalar adder output
	wire [63:0] s_log_out;  //64-bit output of scalar logical unit
	wire [63:0] s_shft_out;  //64-bit output of scalar shift unit
	wire [63:0] s_const_out;  //64-bit output of Scalar constant generator / way to get data from A regs to scalar regs
	wire [63:0] s_imm_out;  //64-bit output of immediate values
	wire [63:0] f_add_out;  //64-bit output of floating point adder unit
	wire [63:0] f_mul_out;  //64-bit output of floating point multiplier unit
	wire [63:0] f_ra_out;  //64-bit output of floating point reciprocal approximation unit

	wire vlog_busy;
	wire vshift_busy;
	wire vadd_busy;
	wire fp_mul_busy;
	wire fp_add_busy;
	wire fp_ra_busy;
	wire vpop_busy;
	wire mem_busy;

	wire [63:0] v_add_out;
	wire [63:0] v_log_out;
	wire [63:0] v_shft_out;

	//vector control registers
	reg [63:0] vector_mask;
	reg [ 6:0] vector_length;

	//scalar register file signals
	wire [ 2:0] s_ex_addr;
	wire [63:0] s_ex_data;
	wire [63:0] s_j_data;  //scalar read data
	wire [63:0] s_k_data;
	wire [63:0] s_i_data;
	reg  [63:0] s_wr_data;  //64-bit data input to scalar register file
	wire        s0_pos;
	wire        s0_neg;
	wire        s0_zero;
	wire        s0_nzero;
	wire [ 7:0] s_res_mask;

	wire [63:0] t_jk_data;
	wire [ 5:0] t_wr_addr;
	wire [ 5:0] t_rd_addr;
	wire [63:0] t_wr_data;
	wire        t_result_en;
	wire        mem_t_wr_en;


	//address register file signals
	wire [ 2:0] a_ex_addr;
	wire [23:0] a_ex_data;
	wire [23:0] a_j_data;  //address read data
	wire [23:0] a_k_data;
	wire [23:0] a_i_data;
	wire [23:0] a_h_data;
	wire [23:0] a_a0_data;
	reg  [23:0] a_wr_data;  //24-bit data input to address register file
	wire        a0_pos;
	wire        a0_neg;
	wire        a0_zero;
	wire        a0_nzero;
	wire [ 7:0] a_res_mask;

	wire [23:0] b_jk_data;
	wire [23:0] b_wr_data;
	wire [ 5:0] b_wr_addr;
	wire [ 5:0] b_rd_addr;
	wire        b_write_en;

	//branch unit signals
	wire [23:0] branch_dest;
	wire        branch_type;
	wire        branch_issue;
	wire        take_branch;
	wire        rtn_jump;

	//memory unit signals

	wire [ 5:0] mem_b_rd_addr;
	wire [ 5:0] mem_b_wr_addr;
	wire        mem_b_wr_en;
	wire [ 5:0] mem_t_rd_addr;
	wire [ 5:0] mem_t_wr_addr;
	wire [63:0] data_from_mem_to_regs;
	wire        mem_type;
	wire        mem_issue;
	wire        mem_ce;
	wire [63:0] data_to_mem;
	wire        mem_wr_en;
	wire [21:0] mem_addr;
	wire        mem_ack;

	//InterCPU signals
	wire intercpu_type;

	//Exchange Package logic
	// exchange sequencer interface
	wire        x_run;  // instructions may issue
	wire        x_swap;  // the exchange sequence owns the registers and the memory port
	wire [ 3:0] x_cnt;  // package word in hand
	wire        x_load;  // load the registers word x_cnt carries from x_data
	wire [63:0] x_data;
	wire        x_done;  // end of the sequence: void the instruction buffers
	wire        x_mem_req;
	wire        x_mem_we;
	wire [21:0] x_mem_addr;
	wire        x_take_exit;  // an exit instruction is taken this clock
	wire        x_take_int;  // an interrupt is taken this clock
	wire        x_request;
	wire        x_idle;
	reg  [23:0] p_save;  // P stored in the outgoing package
	reg  [23:0] cip_addr;  // parcel address of the instruction in CIP
	wire        mem_idle;
	wire        mem_range_err;  // a data reference outside the field was dropped
	wire        p_oof;  // the fetch pointer is outside the field
	wire        nip_in_vld;  // a parcel (or the news that there is none) is ready for NIP
	reg nip_fault, cip_fault;
	wire        fetch_fault;  // the current instruction, or its second parcel, lies outside the field
	wire        branch_range_err;
	wire [23:0] p_mask = XMP ? 24'hFFFFFF : 24'h3FFFFF;  // P is 22 bits on a CRAY-1


	reg  [ 7:0] xa;
	reg  [ 2:0] cln;
	wire        program_state = 1'b0;  //X-MP program state bit: not implemented
	reg  [23:0] instr_base_addr;
	reg  [23:0] instr_limit_addr;
	reg  [23:0] data_base_addr;
	reg  [23:0] data_limit_addr;

	///////////////////////////////////
	//      Mode Register            //
	///////////////////////////////////                                        
	reg mode_ws;  //Waiting for semaphore:           bit28 - w1                                 
	reg mode_fps;  //floating point error status:     bit27 - w1    
	reg mode_bdm;  //bidirectional memory mode:       bit26 - w1  
	reg mode_imm;  //interrupt monitor mode:          bit24 - w1                                         
	reg mode_ior;  //operand range error mode:        bit28 - w2                                      
	reg mode_icm;  //correctable memory error mode:   bit27 - w2 
	reg mode_ifp;  //floating point error mode:       bit26 - w2                                       
	reg mode_ium;  //uncorrectable memory error mode: bit25 - w2 
	reg mode_mm;  //monitor mode:                    bit24 - w2


	/////////////////////////////////
	//      Flag Register    // 
	///////////////////////////////// 
	wire [10:0] flags;
	//individual 1-bit flags
	reg flag_icp;  //Interrupt from Internal CPU
	reg flag_dl;  //deadlock
	reg flag_pci;  //programmable clock interrupt
	reg flag_mcu;  //MCU - set when MIOP send signal
	reg flag_fpe;  //floating point error
	reg flag_ore;  //operand range error - set when data ref is outside DBA/DLA bounds, & en_op_range_interrupt is set
	reg flag_pre;  //program range error - set when instr fetch is outside IBA/ILA bounds
	reg flag_me;  //memory error
	reg flag_ioi;  //i/o interrupt flag
	reg flag_eex;  //set by error exit instr (000)
	reg flag_nex;  //set by normal exit instr (004)

	wire signal_interrupt;

	//Real Time Clock
	reg [63:0] real_time_clock;

	//Programmable Clock
	reg         prog_clock_en;
	wire        clear_prog_clk_int_req;
	wire        set_prog_clk_int_req;
	reg  [31:0] icd;  //interrupt countdown counter
	reg  [31:0] ii;  //Interrupt interval register

	//******************************************
	//*           Instruction Issue            *
	//*                Logic                   *
	//******************************************

	reg [23:0] p_addr;
	reg [15:0] nip, lip, cip;
	reg nip_vld, lip_vld, cip_vld;

	wire [6:0] cip_instr;
	wire [2:0] cip_i, cip_j, cip_k, cip_h;
	wire       issue_vld;
	reg  [5:0] last_instr;
	wire two_parcel_nip, two_parcel_cip;

	//break out the current instruction parcel into fields
	assign cip_instr = cip[15:9];
	assign cip_i     = cip[8:6];
	assign cip_j     = cip[5:3];
	assign cip_k     = cip[2:0];
	assign cip_h     = cip[11:9];

	//A-type scheduler signals
	wire [3:0] a_result_src;
	wire       a_result_en;
	wire       a_wr_en;
	wire [2:0] a_result_dest;  //the a-register we're targeting
	wire [2:0] a_wr_addr;
	wire       a_issue;
	wire       a_type;
	wire       a0_busy;

	//S-type scheduler signals
	wire [4:0] s_result_src;
	wire       s_result_en;
	wire       s_wr_en;
	wire [2:0] s_result_dest;  //the s-register we're targeting
	wire [2:0] s_wr_addr;
	wire       s_issue;
	wire       s_type;
	wire [7:0] vreg_swrite;
	wire [7:0] vreg_swrite_raw;
	wire       s0_busy;

	//V-type scheduler signals
	wire [       3:0] v_fu_delay;
	wire [       7:0] vwrite_start;
	wire [       7:0] vread_start;
	wire [       7:0] vfu_start;
	wire [       7:0] vreg_busy;
	wire [       7:0] vreg_chain_n;
	wire [       7:0] vfu_busy;
	wire [(64*8-1):0] v_rd_data;
	wire              v_issue;
	wire              v_type;

	wire exchange_type;

	// DMA signals
	wire dma_type;
	wire dma_issue;

	//single_stepping
	wire instructions_in_flight;
	reg  single_step;
	wire ok_to_run;
	wire cip_go;  // CIP holds an instruction that may start (not displaced by an interrupt)
	wire [7:0] rd_a, rd_s;  // registers the current instruction reads
	wire opnd_busy;  // one of them still has a result on its way
	wire cip_issue;  // the current instruction issues this clock

	//The instruction as the schedulers see it.  On a CRAY-1 the fields the manual marks x
	//are ignored (023ijx, 026ijx, 027ijx, 072ixx, 073ixx); the X-MP gave those encodings
	//other meanings, which the lookup tables still hold for XMP = 1.
	reg [15:0] cip_dec;
	always @* begin
		cip_dec = cip;
		if (!XMP)
			case (cip[15:9])
				7'o023, 7'o026, 7'o027: cip_dec = {cip[15:3], 3'b000};
				7'o072, 7'o073:         cip_dec = {cip[15:6], 6'b000000};
				default:                ;
			endcase
	end

	always @(posedge clk) single_step <= i_single_step;

	assign instructions_in_flight = (|a_res_mask) || (|s_res_mask) || (|vreg_busy) || (|vfu_busy) || vm_pending;
	//single stepping: an instruction starts only when nothing is in flight.  A memory
	//instruction that was started that way still has to be allowed to finish.
	assign ok_to_run              = !single_step || !instructions_in_flight;
	assign cip_go                 = cip_vld && !x_take_int && ok_to_run && !opnd_busy && !fetch_fault;
	assign cip_issue              = cip_vld && issue_vld;

	cray_opnd opnd (
		.i_cip (cip),
		.o_rd_a(rd_a),
		.o_rd_s(rd_s)
	);
	//076 reads a V register that must not be in use; 003, 073 and the merges 146 and 147
	//wait for a 175 to finish building the mask, and 003 for a merge to finish using it
	wire vec_hold = ((cip_instr==7'o076) && vreg_busy[cip_j]) ||
				(vm_pending && ((cip_instr==7'o003) || (cip_instr==7'o073) || (cip_instr==7'o146) || (cip_instr==7'o147) || (cip_instr==7'o175))) ||
				((cip_instr==7'o003) && tk_busy[0]) ||
	//a scalar floating point instruction waits for a vector operation to leave its unit
	(((cip_instr==7'o062) || (cip_instr==7'o063)) && tk_busy[4]) ||
				((cip_instr[6:2]==5'b01101) && tk_busy[3]) ||
				((cip_instr==7'o070) && tk_busy[5]);
	assign opnd_busy = (|(rd_a & a_res_mask)) || (|(rd_s & s_res_mask)) || vec_hold;

	/////////////////////////////////////
	//    Logic Analyzer        //
	/////////////////////////////////////
	assign o_debug[31:0] = {xa[7:0], 2'b0, cip_vld && issue_vld, cln[2:0], cip_vld, issue_vld, o_p_addr[15:0]};

	//////////////////////////////////////////
	//     Exchange Package Logic           //
	//////////////////////////////////////////
	// The CPU is either running or exchanging.  An exit instruction or an interrupt
	// asks for an exchange; issue stops at once, the parcels behind it are thrown
	// away, everything already issued runs to completion, and then exchange_ctl
	// swaps the registers with the package at XA.

	assign exchange_type = ((cip[15:9] == 7'o000) || (cip[15:9] == 7'o004)) && cip_vld && !cip_fault;

	// An exit is taken as soon as it is the current instruction.  An interrupt is
	// taken between instructions, and never while a memory instruction is under way.
	assign x_take_exit = x_run && exchange_type;
	assign x_take_int  = x_run && signal_interrupt && !exchange_type && mem_idle;
	assign x_request   = x_take_exit || x_take_int;

	assign x_idle = !(|a_res_mask) && !(|s_res_mask) && !(|vreg_busy) && !(|vfu_busy) && !vm_pending && mem_idle && !i_ibuf_busy;

	// The program field.  A parcel whose absolute word address is at or beyond the limit
	// (or beyond the one million words of memory) is not fetched; it travels down the
	// parcel pipeline marked as a fault, and when it would become the current
	// instruction the program range flag sets instead.  The flag cannot set in monitor
	// mode, where the manual does not say what happens; there the fetch is not checked.
	wire [20:0] fetch_limit = (|instr_limit_addr[23:20]) ? 21'h100000 : {1'b0, instr_limit_addr[19:0]};
	wire [22:0] fetch_word = {1'b0, p_addr[23:2]} + {1'b0, instr_base_addr[21:0]};
	assign p_oof       = !mode_mm && (fetch_word >= {2'b0, fetch_limit});
	assign nip_in_vld  = i_nip_vld || p_oof;
	assign fetch_fault = cip_vld && x_run && (cip_fault || (two_parcel_cip && nip_fault));

	//a branch to an address that does not fit P (manual 4-4)
	assign branch_range_err = !XMP && issue_vld && take_branch && (|branch_dest[23:22]);

	assign o_ibuf_hold = !x_run || p_oof;

	exchange_ctl xctl (
		.clk        (clk),
		.rst        (rst),
		.i_request  (x_request),
		.i_idle     (x_idle),
		.i_xa       (xa),
		.o_run      (x_run),
		.o_swap     (x_swap),
		.o_cnt      (x_cnt),
		.o_load     (x_load),
		.o_data     (x_data),
		.o_done     (x_done),
		.o_mem_req  (x_mem_req),
		.o_mem_we   (x_mem_we),
		.o_mem_addr (x_mem_addr),
		.i_mem_ack  (i_mem_ack),
		.i_mem_rdata(i_data_from_mem)
	);

	assign o_clear_ibufs = x_done;

	// P saved in the outgoing package: the parcel after an exit (manual 4-7, 4-13),
	// or the first instruction that has not issued when an interrupt is taken.
	always @(posedge clk)
		if (rst) p_save <= 24'b0;
		else if (x_take_exit) p_save <= (cip_addr + 24'd1) & p_mask;
		else if (x_take_int) p_save <= cip_vld ? cip_addr : (nip_vld ? ((p_addr - 24'd1) & p_mask) : p_addr);

	//look up the appropriate A and S reg values to store them during the exchange sequence
	assign a_ex_addr = x_cnt[2:0];
	assign s_ex_addr = x_cnt[2:0];

	//The instruction buffers always follow the program counter
	assign o_p_addr = p_addr + {instr_base_addr[21:0], 2'b0};

	// Outgoing package word.
	// CRAY-1 layout (manual figure 3-8):
	//   word 0  P [45:24]                          A0 [23:0]
	//   word 1  BA [45:28]                         A1
	//   word 2  LA [45:28]  M [27:24]              A2
	//   word 3  XA [47:40]  VL [39:33]  F [32:24]  A3
	//   words 4-7  A4-A7,  words 8-15  S0-S7
	reg [63:0] x_word;
	always @* begin
		if (XMP)
			case (x_cnt)
				4'b0000: x_word = {i_cpu_num[1:0], 14'b0, p_save, a_ex_data};
				4'b0001:
				x_word = {16'b0, instr_base_addr[23:5], mode_ws, mode_fps, mode_bdm, 1'b0, mode_imm, a_ex_data};
				4'b0010:
				x_word = {16'b0, instr_limit_addr[23:5], mode_ior, mode_icm, mode_ifp, mode_ium, mode_mm, a_ex_data};
				4'b0011: x_word = {14'b0, flags[10:9], xa, vector_length, flags[8:0], a_ex_data};
				4'b0100: x_word = {16'b0, data_base_addr[23:6], 1'b0, program_state, 1'b0, cln[2:0], a_ex_data};
				4'b0101: x_word = {16'b0, data_limit_addr[23:6], 6'b0, a_ex_data};
				4'b0110: x_word = {40'b0, a_ex_data};
				4'b0111: x_word = {40'b0, a_ex_data};
				default: x_word = s_ex_data;
			endcase
		else
			case (x_cnt)
				4'b0000: x_word = {18'b0, p_save[21:0], a_ex_data};
				4'b0001: x_word = {18'b0, instr_base_addr[21:4], 4'b0, a_ex_data};
				4'b0010: x_word = {18'b0, instr_limit_addr[21:4], mode_icm, mode_ifp, mode_ium, mode_mm, a_ex_data};
				4'b0011: x_word = {16'b0, xa, vector_length, flags[8:0], a_ex_data};
				4'b0100: x_word = {40'b0, a_ex_data};
				4'b0101: x_word = {40'b0, a_ex_data};
				4'b0110: x_word = {40'b0, a_ex_data};
				4'b0111: x_word = {40'b0, a_ex_data};
				default: x_word = s_ex_data;
			endcase
	end

	always @* o_data_to_mem = x_swap ? x_word : data_to_mem;

	//The data memory port belongs to the exchange sequence while it swaps
	assign o_mem_wr_en = x_swap ? x_mem_we : mem_wr_en;
	assign o_mem_ce    = x_swap ? x_mem_req : mem_ce;
	assign o_mem_addr  = x_swap ? x_mem_addr : mem_addr;
	assign mem_ack     = i_mem_ack && !x_swap;

	//Set up the instr/data base and limit registers: bits 16-33, words 1,2 and 4,5
	always @(posedge clk)
		if (rst) instr_base_addr <= 24'b0;
		else if (x_load && (x_cnt == 4'b0001))
			instr_base_addr <= XMP ? {x_data[47:29], 5'b0} : {2'b0, x_data[45:28], 4'b0};

	always @(posedge clk)
		if (rst) instr_limit_addr <= 24'hFFFFFF;
		else if (x_load && (x_cnt == 4'b0010))
			instr_limit_addr <= XMP ? {x_data[47:29], 5'b0} : {2'b0, x_data[45:28], 4'b0};

	always @(posedge clk)
		if (rst) data_base_addr <= 24'b0;
		else if (XMP ? (x_load && (x_cnt == 4'b0100)) : (x_load && (x_cnt == 4'b0001)))
			data_base_addr <= XMP ? {x_data[47:29], 5'b0} : {2'b0, x_data[45:28], 4'b0};

	always @(posedge clk)
		if (rst) data_limit_addr <= 24'hFFFFFF;
		else if (XMP ? (x_load && (x_cnt == 4'b0101)) : (x_load && (x_cnt == 4'b0010)))
			data_limit_addr <= XMP ? {x_data[47:29], 5'b0} : {2'b0, x_data[45:28], 4'b0};


	//and exchange address
	always @(posedge clk)
		if (rst) xa <= 8'h0;
		else if (x_load && (x_cnt == 4'b0011)) xa <= x_data[47:40];
		else if (!x_swap)
			xa <= ((cip[15:6]==10'o0013) && (XMP ? (cip[2:0]==3'b0) : 1'b1) && cip_issue && mode_mm) ? a_j_data[11:4] : xa;   //(Aj) is 0 when j is 0

	//Set the mode bits - Pg 3-9 of CSM-0111000
	always @(posedge clk)
		if (rst) begin
			mode_ws  <= 1'b0;
			mode_fps <= 1'b0;
			mode_bdm <= 1'b0;
			mode_imm <= 1'b0;
			mode_ior <= 1'b0;
			mode_icm <= 1'b0;
			mode_ifp <= 1'b0;
			mode_ium <= 1'b0;
			mode_mm  <= 1'b0;
		end else if (XMP && x_load && (x_cnt == 4'b0001)) begin
			mode_ws  <= x_data[28];
			mode_fps <= x_data[27];
			mode_bdm <= x_data[26];
			mode_imm <= x_data[24];
		end else if (x_load && (x_cnt == 4'b0010)) begin
			mode_ior <= x_data[28];
			mode_icm <= x_data[27];
			mode_ifp <= x_data[26];
			mode_ium <= x_data[25];
			mode_mm  <= x_data[24];
		end else if (!XMP && cip_issue && (cip[15:6] == 10'o0021)) mode_ifp <= 1'b1;
		else if (!XMP && cip_issue && (cip[15:6] == 10'o0022)) mode_ifp <= 1'b0;



	//Now configure all of the flag bits
	always @(posedge clk)
		if (rst) begin
			flag_icp <= 1'b0;
			flag_dl  <= 1'b0;
			flag_pci <= 1'b0;
			flag_mcu <= 1'b0;
			flag_fpe <= 1'b0;
			flag_ore <= 1'b0;
			flag_pre <= 1'b0;
			flag_me  <= 1'b0;
			flag_ioi <= 1'b0;
			flag_eex <= 1'b0;
			flag_nex <= 1'b0;
		end  //Load initial values from the exchange package.
		else if (x_load && (x_cnt == 4'b0011)) begin
			flag_icp <= XMP ? x_data[49] : 1'b0;  //bit 14
			flag_dl  <= XMP ? x_data[48] : 1'b0;  //bit 15
			flag_pci <= x_data[32];  //bit 31
			flag_mcu <= x_data[31];  //bit 32
			flag_fpe <= x_data[30];  //bit 33
			flag_ore <= x_data[29];  //bit 34
			flag_pre <= x_data[28];  //bit 35
			flag_me  <= x_data[27];  //bit 36
			flag_ioi <= x_data[26];  //bit 37
			flag_eex <= x_data[25];  //bit 38
			flag_nex <= x_data[24];  //bit 39
		end  //Now we need to take care of the conditions that actually set all of these flags
		else if (!x_swap) begin
			//Interrupt from Internal CPU - Set when another CPU issues instr 001401
			flag_icp <= 1'b0;
			//Deadlock - set when all CPUs in a cluster are holding issue on a test & set instr
			flag_dl <= 1'b0;
			//Programmable clock interrupt - set when the interrupt countdown counter in
			//the programmable clock equals 0.
			flag_pci <= mode_mm ? 1'b0 :
							XMP     ? ((flag_pci || set_prog_clk_int_req) && !clear_prog_clk_int_req) :
									  (flag_pci || i_console_int);   //the console interrupt on a CRAY-1
			//MCU interrupt - set when the MIOP sends this signal
			flag_mcu <= 1'b0;
			//Floating Point Error - set when the floating point range error occurs in any of
			//the floating-point functional units and the enable floating-point interrupt flag is set. 
			flag_fpe <= mode_mm ? 1'b0 : (flag_fpe || (mode_ifp && fp_range_err));
			//Operand Range Error - set when the data reference is made outside the boundaries of 
			//the data base address and data limit address registers, and the Enable Operand Range
			//Interrupt flag is set. 
			flag_ore <= mode_mm ? 1'b0 : (flag_ore || mem_range_err);
			//Program Range Error - set when an instruction fetch is made outside the boundaries of 
			//the Instruction Base Address and Instruction Limit Address registers.
			flag_pre <= mode_mm ? 1'b0 : (flag_pre || fetch_fault || branch_range_err);
			//Memory Error - set when a correctable or uncorrectable memory error occurs and the
			//corresponding enable memory error mode bit is set in the M register
			flag_me <= 1'b0;
			//I/O Interrupt flag - set when a 6 Mbyte channel or the 1250 Mbyte channel completes a transfer
			flag_ioi <= mode_mm ? 1'b0 : i_dma_int;
			//Error Exit - set by an error exit instruction (000)
			flag_eex <= mode_mm ? 1'b0 : (((cip[15:9] == 7'o000) && cip_vld && issue_vld) || flag_eex);
			//Normal Exit - set by a normal exit instruction (004)
			flag_nex <= mode_mm ? 1'b0 : (((cip[15:9] == 7'o004) && cip_vld && issue_vld) || flag_nex);
		end

	//Monitor_mode (mode_mm) inhibits all interrupts except memory errors, error exit or normal exit
	//Interrupt Monitor Mode (mode_imm) re-enables everything except PC, MCU, I/O and ICP errors
	//Interrupt sources
	//Deadlock: !mode_mm || mode_imm
	//PCI: !mode_mm
	//MCU: !mode_mm
	//FPE: !mode_mm || mode_imm
	//ORE: !mode_mm || mode_imm
	//PRE: !mode_mm || mode_imm
	//ME:  1'b1
	//IOI: !mode_mm
	//EEX: 1'b1
	//NEX: 1'b1

	//page 3-12 of Cray XMP-1 system programmer reference manual
	//Non ME flags can only be set if not in monitor mode
	//Except for the ME flag, if the program is in monitor mode
	//and the conditions for setting an F register are present, the
	//flag remains cleared and no exchange sequence is initiated.

	assign flags[10:0] = {
		flag_icp, flag_dl, flag_pci, flag_mcu, flag_fpe, flag_ore, flag_pre, flag_me, flag_ioi, flag_eex, flag_nex
	};

	//Fire an interrupt when the current instruction executes, we're not in monitor mode, and a flag has been set	
	assign signal_interrupt = x_run && |flags[10:0] && !mode_mm;

	//Cluster Number
	always @(posedge clk)
		if (rst) cln <= 3'b0;
		else if (XMP && x_load && (x_cnt == 4'b0100)) cln <= x_data[26:24];
		else if ((cip[15:6] == 10'b0000001100) && (cip[2:0] == 3'b011) && cip_vld) cln <= cip[5:3];

	//1) accept the incoming data from the instruction buffers
	always @(posedge clk)
		if (rst || x_request || x_done) begin
			nip_fault <= 1'b0;
			cip_fault <= 1'b0;
			nip       <= 16'b0;
			cip       <= 16'b0;
			lip       <= 16'b0;
			nip_vld   <= 1'b0;
			cip_vld   <= 1'b0;
			lip_vld   <= 1'b0;
		end else if (nip_in_vld && issue_vld) begin
			nip_fault <= p_oof;
			cip_fault <= nip_fault && nip_vld && !take_branch;
			cip_addr <= (p_addr - 24'd1) & p_mask;  // NIP holds the parcel before the fetch pointer
			nip <= i_nip_nxt;
			lip <= i_nip_nxt;
			cip <= nip;
			lip_vld <= take_branch ? 1'b0 : two_parcel_nip; //1'b1; //nip_vld;   //Only set it if you have a two_parcel_nip, and you haven't just branched
			nip_vld <= (two_parcel_nip || take_branch) ? 1'b0 : 1'b1;            //Set it if you didn't branch and the current cycle holds a one parcel nip
			cip_vld <= take_branch ? 1'b0 : nip_vld;
		end  //To catch the case where instruction issues during an I-cache miss.
			 //Set cip_vld=0, preserve everything else.
		else if (issue_vld) begin
			nip     <= nip;
			lip     <= lip;
			cip     <= 16'b0;
			lip_vld <= 1'b0;  //lip_vld;
			nip_vld <= take_branch ? 1'b0 : nip_vld;  //nip_vld;
			cip_vld <= 1'b0;
		end

	//Two parcel instructions
	assign two_parcel_nip = nip_vld && !nip_fault && ((nip[15:10] == 6'b000011) ||  //006-007
		(nip[15:12] == 4'b0001) ||  //010-017
		(nip[15:10] == 6'b001000) ||  //020-021
		(nip[15:10] == 6'b010000) ||  //040-041
		(nip[15:14] == 2'b10));  //100-137


	assign two_parcel_cip = cip_vld && !cip_fault && ((cip[15:10] == 6'b000011) ||  //006-007
		(cip[15:12] == 4'b0001) ||  //010-017
		(cip[15:10] == 6'b001000) ||  //020-021
		(cip[15:10] == 6'b010000) ||  //040-041
		(cip[15:14] == 2'b10));  //100-137
	always @(posedge clk) last_instr <= cip[5:0];

	//Track S-type related reservations, destination data and if we can issue or not
	s_scheduler ssched (
		.clk            (clk),
		.rst            (rst),
		.i_cip          (cip_dec),
		.i_cip_vld      (cip_vld),
		.i_issue_vld    (issue_vld),
		.o_s_issue      (s_issue),
		.o_s_result_en  (s_result_en),
		.o_s_result_src (s_result_src),
		.o_s_result_dest(s_result_dest),
		.o_s_type       (s_type),
		.i_vreg_busy    (vreg_busy),
		.o_vreg_write   (vreg_swrite_raw),
		.o_s0_busy      (s0_busy),
		.o_s_res_mask   (s_res_mask)
	);

	//Now let's get the correct data to write
	always @* begin
		if (!x_swap)
			case (s_result_src[4:0])
				SBUS_IMM: s_wr_data = s_imm_out;
				SBUS_COMP_IMM: s_wr_data = s_imm_out;
				SBUS_S_LOG: s_wr_data = s_log_out;
				SBUS_S_SHIFT: s_wr_data = s_shft_out;
				SBUS_S_ADD: s_wr_data = s_add_out;
				SBUS_FP_ADD: s_wr_data = f_add_out;
				SBUS_FP_MULT: s_wr_data = f_mul_out;
				SBUS_FP_RA: s_wr_data = f_ra_out;
				SBUS_CONST_GEN: s_wr_data = s_const_out;
				SBUS_RTC: s_wr_data = real_time_clock;
				SBUS_V_MASK: s_wr_data = vector_mask;
				SBUS_T_BUS: s_wr_data = t_jk_data;
				SBUS_V0: s_wr_data = v_rd_data[63:0];
				SBUS_V1: s_wr_data = v_rd_data[127:64];
				SBUS_V2: s_wr_data = v_rd_data[191:128];
				SBUS_V3: s_wr_data = v_rd_data[255:192];
				SBUS_V4: s_wr_data = v_rd_data[319:256];
				SBUS_V5: s_wr_data = v_rd_data[383:320];
				SBUS_V6: s_wr_data = v_rd_data[447:384];
				SBUS_V7: s_wr_data = v_rd_data[511:448];
				SBUS_MEM: s_wr_data = data_from_mem_to_regs;
				SBUS_INTERCPU:
				s_wr_data = XMP ? i_intercpu_si : real_time_clock;  //072: the clock is in the CPU on a CRAY-1
				SBUS_HI_SR: s_wr_data = {22'b0, i_cpu_num[1:0], 5'b0, cln[2:0], 32'b0};
				default: s_wr_data = 64'b0;
			endcase
		else s_wr_data = x_data;
	end

	assign s_wr_en   = !x_swap ? s_result_en : (x_load && x_cnt[3]);
	assign s_wr_addr = !x_swap ? s_result_dest : s_ex_addr;


	//Track A-type related reservations, destination data and if we can issue or not
	a_scheduler asched (
		.clk               (clk),
		.rst               (rst),
		.i_cip             (cip_dec),
		.i_cip_vld         (cip_vld),
		.i_lip_vld         (lip_vld),
		.i_issue_vld       (issue_vld),
		.i_total_s_res_mask(s_res_mask),
		.o_a_issue         (a_issue),
		.o_a_result_en     (a_result_en),
		.o_a_result_src    (a_result_src),
		.o_a_result_dest   (a_result_dest),
		.o_a_type          (a_type),
		.o_a0_busy         (a0_busy),
		.o_a_res_mask      (a_res_mask)
	);


	always @* begin
		if (!x_swap)
			case (a_result_src[3:0])
				ABUS_IMM:      a_wr_data = a_imm_out;
				ABUS_COMP_IMM: a_wr_data = a_imm_out;
				ABUS_SIMM:     a_wr_data = {18'b0, last_instr};
				ABUS_S_BUS:    a_wr_data = a_imm_out;
				ABUS_B_BUS:    a_wr_data = b_jk_data;
				ABUS_S_POP:    a_wr_data = a_poplz_out;
				ABUS_A_ADD:    a_wr_data = a_add_out;
				ABUS_A_MULT:   a_wr_data = a_mul_out;
				ABUS_CHANNEL:  a_wr_data = i_dma_ai[23:0];
				ABUS_MEM:      a_wr_data = data_from_mem_to_regs[23:0];
				ABUS_INTERCPU: a_wr_data = i_intercpu_ai;
				default:       a_wr_data = 24'b0;
			endcase
		else a_wr_data = x_data[23:0];
	end

	assign a_wr_en   = !x_swap ? a_result_en : (x_load && !x_cnt[3]);
	assign a_wr_addr = !x_swap ? a_result_dest : a_ex_addr;

	//Track V-type instructions
	v_scheduler #(
		.XMP(XMP)
	) vsched (
		.i_cip         (cip),
		.i_cip_vld     (cip_go),
		.i_a_res_mask  (a_res_mask),
		.o_fu_delay    (v_fu_delay),
		.o_fu          (),
		.o_vwrite_start(vwrite_start),
		.o_vread_start (vread_start),
		.o_vfu_start   (vfu_start),
		.o_v_issue     (v_issue),
		.i_vreg_busy   (vreg_busy),
		.i_vreg_chain_n(vreg_chain_n),
		.i_vfu_busy    (vfu_busy)
	);
	/*
localparam VLOG      = 3'b000,   //vector logical
           VSHIFT    = 3'b001,	 //vector shift
			  VADD      = 3'b010,
	        FP_MUL    = 3'b011,   //FP multiply
	        FP_ADD    = 3'b100,   //FP adder 
	        FP_RA     = 3'b101,   //FP recip. approx.
	        VPOP      = 3'b110,   //vector pop count / parity
	         MEM      = 3'b111;
				
				vfu_start
*/
	assign vreg_swrite = vreg_swrite_raw & {8{cip_issue}};

	assign vfu_busy[0] = vlog_busy;
	assign vfu_busy[1] = vshift_busy;
	assign vfu_busy[2] = vadd_busy;
	assign vfu_busy[3] = fp_mul_busy;
	assign vfu_busy[4] = fp_add_busy;
	assign vfu_busy[5] = fp_ra_busy;
	assign vfu_busy[6] = vpop_busy;
	assign vfu_busy[7] = mem_busy;
	assign mem_idle    = !mem_busy;
	assign v_type      = (cip[15:14] == 2'b11);

	//check if it's free to issue

	assign issue_vld = (
						 (s_issue && s_type && mem_issue && mem_type) ||
						 (s_issue && s_type && !mem_type) ||
						 (a_issue && a_type && mem_issue && mem_type) ||
						 (a_issue && a_type && !mem_type) ||
						 (v_type && mem_type && mem_issue) ||
						 (v_issue && v_type && !mem_type) ||
						 (branch_issue && branch_type) ||
						 (mem_issue && mem_type && !s_type && !a_type) ||
						 (i_intercpu_issue && intercpu_type) ||
						 (dma_issue && dma_type) || 
						 exchange_type ||
						 !(s_type || a_type || v_type || branch_type || mem_type || exchange_type || intercpu_type || dma_type) ||
						 !cip_vld) && (ok_to_run || (mem_type && mem_issue)) && x_run && !x_take_int && !(cip_vld && opnd_busy) && !fetch_fault;



	//////////////////////////////////////////////////
	//           Register Files                     //
	//////////////////////////////////////////////////

	//The vector registers and the bookkeeping for vector operations in progress.
	//
	//A vector instruction issues once (v_issue).  From then on its functional unit's
	//tracker (v_optrack) says in which clocks operands are at the unit and results
	//come out, and each V register follows its own read or write role.

	//Number of elements a vector instruction processes: VL of 0 means 64 (manual 4-10)
	wire [6:0] vl_count = (vector_length[5:0] == 6'd0) ? 7'd64 : {1'b0, vector_length[5:0]};
	wire [4:0] v_rec_delay = {1'b0, v_fu_delay} + 5'd2;  //recursive operand delay: unit time + 2

	//units with a tracker: 0 logical, 1 shift, 2 integer add, 3 FP multiply, 4 FP add, 5 reciprocal
	wire [ 5:0] tk_busy;
	wire [15:0] tk_instr     [0:5];
	wire [63:0] tk_sj        [0:5];
	wire [23:0] tk_ak        [0:5];
	wire [ 5:0] tk_in_valid;
	wire [ 5:0] tk_in_idx    [0:5];
	wire [ 5:0] tk_in_first;
	wire [ 5:0] tk_in_last;
	wire [ 5:0] tk_out_valid;
	wire [ 5:0] tk_out_idx   [0:5];
	wire [ 5:0] tk_out_last;
	wire [ 2:0] tk_out_dest  [0:5];
	wire [ 5:0] tk_out_wr_v;
	wire [63:0] fu_out       [0:5];

	assign fu_out[0] = v_log_out;
	assign fu_out[1] = v_shft_out;
	assign fu_out[2] = v_add_out;
	assign fu_out[3] = f_mul_out;
	assign fu_out[4] = f_add_out;
	assign fu_out[5] = f_ra_out;

	genvar gu;
	generate
		for (gu = 0; gu < 6; gu = gu + 1) begin : g_track
			v_optrack #(
				.L((gu == 0) ? 2 : (gu == 1) ? 4 : (gu == 2) ? 3 : (gu == 3) ? 7 : (gu == 4) ? 6 : 14)
			) track (
				.clk        (clk),
				.rst        (rst),
				.i_start    (vfu_start[gu]),
				.i_len      (vl_count),
				.i_cip      (cip),
				.i_sj       (s_j_data),
				.i_ak       (a_k_data),
				.i_wr_v     (|vwrite_start),
				.o_busy     (tk_busy[gu]),
				.o_instr    (tk_instr[gu]),
				.o_sj       (tk_sj[gu]),
				.o_ak       (tk_ak[gu]),
				.o_in_valid (tk_in_valid[gu]),
				.o_in_idx   (tk_in_idx[gu]),
				.o_in_first (tk_in_first[gu]),
				.o_in_last  (tk_in_last[gu]),
				.o_out_valid(tk_out_valid[gu]),
				.o_out_idx  (tk_out_idx[gu]),
				.o_out_last (tk_out_last[gu]),
				.o_out_dest (tk_out_dest[gu]),
				.o_out_wr_v (tk_out_wr_v[gu])
			);
		end
	endgenerate

	//a vector load (176) writes one element per word read; a vector store (177) reads
	//the element the memory unit is about to write
	wire       vmem_wr;  //o_mem_data holds element vmem_wr_idx of a 176
	wire [5:0] vmem_wr_idx;
	wire [5:0] vmem_rd_idx;
	wire       vmem_store = mem_busy && (cip_instr == 7'o177);

	genvar gr;
	generate
		for (gr = 0; gr < 8; gr = gr + 1) begin : g_vreg
			//who writes this register now: a functional unit, a vector load or a 077
			reg            wr_en;
			reg     [ 5:0] wr_idx;
			reg     [63:0] wr_data;
			integer        u;
			always @* begin
				wr_en   = 1'b0;
				wr_idx  = a_k_data[5:0];
				wr_data = s_j_data;
				if (vreg_swrite[gr])  //077: (Sj) to element (Ak)
					wr_en = 1'b1;
				if (vmem_wr && (cip_i == gr)) begin
					wr_en   = 1'b1;
					wr_idx  = vmem_wr_idx;
					wr_data = data_from_mem_to_regs;
				end
				for (u = 0; u < 6; u = u + 1)
				if (tk_out_valid[u] && tk_out_wr_v[u] && (tk_out_dest[u] == gr)) begin
					wr_en   = 1'b1;
					wr_idx  = tk_out_idx[u];
					wr_data = fu_out[u];
				end
			end

			v_regfile vreg (
				.clk        (clk),
				.rst        (rst),
				.i_rd_start (vread_start[gr] && !mem_type),
				.i_len      (vl_count),
				.i_recursive(vwrite_start[gr]),
				.i_rec_delay(v_rec_delay),
				.i_elem_idx (a_k_data[5:0]),
				.i_mem_rd   (vmem_store && (cip_j == gr)),
				.i_mem_idx  (vmem_rd_idx),
				.o_rd_data  (v_rd_data[64*gr+:64]),
				.i_wr_start (vwrite_start[gr]),
				.i_wr_en    (wr_en),
				.i_wr_idx   (wr_idx),
				.i_wr_data  (wr_data),
				.o_busy     (vreg_busy[gr])
			);
		end
	endgenerate

	assign vreg_chain_n = 8'hFF;  //results are not chained into a following operation yet

	//operands for the units, picked by the instruction each tracker holds
	function [63:0] vreg_sel;
		input [511:0] all;
		input [2:0] n;
		begin
			vreg_sel = all[64*n+:64];
		end
	endfunction

	s_regfile #(
		.WIDTH   (64),
		.DEPTH   (8),
		.LOGDEPTH(3)
	) s_rf (
		.clk       (clk),
		.rst       (rst),
		.i_j_addr  (cip_j),
		.i_k_addr  (cip_k),
		.i_i_addr  (cip_i),
		.i_ex_addr (s_ex_addr),
		.o_ex_data (s_ex_data),
		.o_j_data  (s_j_data),
		.o_k_data  (s_k_data),
		.o_i_data  (s_i_data),
		.i_wr_addr (s_wr_addr),
		.i_wr_data (s_wr_data),
		.i_wr_en   (s_wr_en),
		.o_s0_pos  (s0_pos),
		.o_s0_neg  (s0_neg),
		.o_s0_zero (s0_zero),
		.o_s0_nzero(s0_nzero)
	);



	t_regfile_hard #(
		.WIDTH   (64),
		.DEPTH   (64),
		.LOGDEPTH(6)
	) t_rf (
		.clk      (clk),
		.i_jk_addr(t_rd_addr),
		.o_jk_data(t_jk_data),
		.i_wr_addr(t_wr_addr),
		.i_wr_data(t_wr_data),
		.i_wr_en  (t_result_en)
	);

	assign t_rd_addr = mem_type ? mem_t_rd_addr : {cip_j, cip_k};
	assign t_wr_addr = mem_type ? mem_t_wr_addr : {cip_j, cip_k};

	//We can accept data from the S reg-file or from memory
	assign t_wr_data   = (cip_instr == 7'o075) ? s_i_data : data_from_mem_to_regs;
	//075 writes (Si) to Tjk when it issues; 036 writes from memory
	assign t_result_en = mem_t_wr_en || (cip_issue && (cip_instr == 7'o075));

	a_regfile #(
		.WIDTH   (24),
		.DEPTH   (8),
		.LOGDEPTH(3)
	) A_rf (
		.clk       (clk),
		.rst       (rst),
		.i_j_addr  (cip_j),
		.i_k_addr  (cip_k),
		.i_i_addr  (cip_i),
		.i_h_addr  (cip_h),
		.i_ex_addr (a_ex_addr),
		.o_ex_data (a_ex_data),
		.o_j_data  (a_j_data),
		.o_k_data  (a_k_data),
		.o_i_data  (a_i_data),
		.o_h_data  (a_h_data),
		.o_a0_data (a_a0_data),
		.i_wr_addr (a_wr_addr),
		.i_wr_data (a_wr_data),
		.i_wr_en   (a_wr_en),
		.o_a0_pos  (a0_pos),
		.o_a0_neg  (a0_neg),
		.o_a0_zero (a0_zero),
		.o_a0_nzero(a0_nzero)
	);


	b_regfile #(
		.WIDTH   (24),
		.DEPTH   (64),
		.LOGDEPTH(6)
	) b_rf (
		.clk       (clk),
		.i_jk_addr (b_rd_addr),
		.o_jk_data (b_jk_data),
		.i_wr_addr (b_wr_addr),
		.i_wr_data (b_wr_data),
		.i_wr_en   (b_write_en),
		.i_cur_p   (p_addr),
		.i_rtn_jump(rtn_jump)
	);

	//Figure out when and what we should write into the B register file
	assign b_wr_addr  = (cip_instr == 7'o025) ? {cip_j, cip_k} : mem_b_wr_addr;
	assign b_wr_data  = (cip_instr == 7'o025) ? a_i_data : data_from_mem_to_regs[23:0];
	assign b_write_en = ((cip_instr == 7'o025) && a_issue && cip_issue) || ((cip_instr == 7'o034) && mem_b_wr_en);

	//and figure out what address to read from
	assign b_rd_addr = (cip_instr == 7'o035) ? mem_b_rd_addr : {cip_j, cip_k};

	//////////////////////////////////////////////////
	//           Vector Units                       //
	//////////////////////////////////////////////////
	//Each unit is a plain pipeline.  The first operand is (Sj) for the even
	//instructions and the Vj element for the odd ones; the second is the Vk element.

	//Vector Logical unit (140-147) and the element test of 175
	wire v_test_out;
	vector_logical vlog (
		.clk     (clk),
		.i_op    (tk_instr[0][11:9]),
		.i_test  (tk_instr[0][1:0]),
		.i_a     (tk_instr[0][9] ? vreg_sel(v_rd_data, tk_instr[0][5:3]) : tk_sj[0]),
		.i_b     (vreg_sel(v_rd_data, tk_instr[0][2:0])),
		.i_vm_bit(vector_mask[6'd63-tk_in_idx[0]]),
		.o_result(v_log_out),
		.o_test  (v_test_out)
	);

	//Vector Shift unit (150-153)
	vector_shift vshift (
		.clk     (clk),
		.i_op    (tk_instr[1][10:9]),
		.i_cnt   (tk_ak[1]),
		.i_d     (vreg_sel(v_rd_data, tk_instr[1][5:3])),
		.i_valid (tk_in_valid[1]),
		.i_first (tk_in_first[1]),
		.i_last  (tk_in_last[1]),
		.o_result(v_shft_out)
	);

	//Vector Add unit (154, 155 sums; 156, 157 differences)
	vector_add vadd (
		.clk     (clk),
		.i_sub   (tk_instr[2][10]),
		.i_a     (tk_instr[2][9] ? vreg_sel(v_rd_data, tk_instr[2][5:3]) : tk_sj[2]),
		.i_b     (vreg_sel(v_rd_data, tk_instr[2][2:0])),
		.o_result(v_add_out)
	);

	assign vpop_busy = 1'b0;

	assign vlog_busy   = tk_busy[0];
	assign vshift_busy = tk_busy[1];
	assign vadd_busy   = tk_busy[2];

	//The vector mask instruction 175 builds VM one element at a time as the tests come
	//out of the logical unit.  Element 0 is mask bit 63; elements not tested are zero.
	reg  vm_pending;  //a 175 has issued and its last test is not in yet
	wire vm_test_out = tk_out_valid[0] && !tk_out_wr_v[0];
	always @(posedge clk)
		if (rst) vm_pending <= 1'b0;
		else if (vfu_start[0] && (cip_instr == 7'o175)) vm_pending <= 1'b1;
		else if (vm_test_out && tk_out_last[0]) vm_pending <= 1'b0;

	///////////////////////////////////////////////
	//          Floating Point Units             //
	///////////////////////////////////////////////


	//Floating Point Addition unit
	// The three floating point units are plain pipelines: operands in during one clock,
	// the result out a fixed number of clocks later (6, 7 and 14).  They are bit-exact
	// with the reference model in tools/crates/fp.  A scalar instruction feeds a unit in
	// the clock it issues; a vector instruction feeds it one element pair per clock while
	// its tracker says so, and no scalar instruction issues to that unit meanwhile.
	wire fp_add_err, fp_mul_err, fp_ra_err;

	//Floating Point Addition unit (062, 063; 170-173)
	wire fa_vec = tk_in_valid[4];
	fp_add fadd (
		.clk        (clk),
		.i_a        (fa_vec ? (tk_instr[4][9] ? vreg_sel(v_rd_data, tk_instr[4][5:3]) : tk_sj[4]) : s_j_data),
		.i_b        (fa_vec ? vreg_sel(v_rd_data, tk_instr[4][2:0]) : s_k_data),
		.i_sub      (fa_vec ? tk_instr[4][10] : cip_instr[0]),
		.o_result   (f_add_out),
		.o_range_err(fp_add_err)
	);

	//Floating Point Multiply unit (064-067; 160-167: full, half-precision, rounded, two minus product)
	wire fm_vec = tk_in_valid[3];
	fp_mul fmult (
		.clk        (clk),
		.i_a        (fm_vec ? (tk_instr[3][9] ? vreg_sel(v_rd_data, tk_instr[3][5:3]) : tk_sj[3]) : s_j_data),
		.i_b        (fm_vec ? vreg_sel(v_rd_data, tk_instr[3][2:0]) : s_k_data),
		.i_kind     (fm_vec ? tk_instr[3][11:10] : cip_instr[1:0]),
		.o_result   (f_mul_out),
		.o_range_err(fp_mul_err)
	);

	//Floating Point Reciprocal Approximation unit (070; 174)
	wire fr_vec = tk_in_valid[5];
	fp_recip frecip (
		.clk        (clk),
		.i_a        (fr_vec ? vreg_sel(v_rd_data, tk_instr[5][5:3]) : s_j_data),
		.o_result   (f_ra_out),
		.o_range_err(fp_ra_err)
	);

	assign fp_mul_busy = tk_busy[3];
	assign fp_add_busy = tk_busy[4];
	assign fp_ra_busy  = tk_busy[5];

	//A floating point range error is reported when the result is delivered
	wire fp_range_err = (s_result_en && (((s_result_src==SBUS_FP_ADD)  && fp_add_err) ||
									 ((s_result_src==SBUS_FP_MULT) && fp_mul_err) ||
									 ((s_result_src==SBUS_FP_RA)   && fp_ra_err))) ||
					(tk_out_valid[4] && fp_add_err) ||
					(tk_out_valid[3] && fp_mul_err) ||
					(tk_out_valid[5] && fp_ra_err);

	//////////////////////////////////////////////////
	//           Scalar Units                       //
	//////////////////////////////////////////////////

	//Scalar Addition unit
	scalar_add sadd (
		.clk     (clk),        //system clock input
		.i_instr (cip_instr),  //7-bit instruction input
		.i_sj    (s_j_data),   //64-bit sj input
		.i_sk    (s_k_data),   //64-bit sk input
		.o_result(s_add_out)
	);  //64-bit output


	//Scalar Logical unit
	scalar_logical slog (
		.clk     (clk),        //system clock input
		.i_instr (cip_instr),  //7-bit instruction input
		.i_j     (cip_j),      //3-bit j input
		.i_k     (cip_k),      //3-bit k input
		.i_sj    (s_j_data),   //64-bit sj input
		.i_sk    (s_k_data),   //64-bit sk input
		.i_si    (s_i_data),   //64-bit si input
		.o_result(s_log_out)
	);  //64-bit output


	//Scalar Population Count and Leading-Zero Count unit
	scalar_pop_lz spoplz (
		.clk     (clk),         //system clock input
		.i_issue (cip_issue),
		.i_instr (cip_instr),   //7-bit instruction input
		.i_sj    (s_j_data),    //64-bit sj input
		.o_result(a_poplz_out)
	);  //24-bit output

	//Scalar Shift unit
	scalar_shift sshift (
		.clk     (clk),
		.i_issue (cip_issue),
		.i_instr (cip_instr),
		.i_j     (cip_j),
		.i_k     (cip_k),
		.i_si    (s_i_data),
		.i_sj    (s_j_data),
		.i_ak    (a_k_data),
		.o_result(s_shft_out)
	);

	s_const_gen sconst (
		.clk     (clk),
		.i_j     (cip_j),
		.i_ak    (a_k_data),
		.o_result(s_const_out)
	);
	//////////////////////////////////////////////////
	//          Address Units                       //
	//////////////////////////////////////////////////

	//This block actually generates immediate values for both
	//address and scalar instructions
	imm_gen igen (
		.clk       (clk),
		.i_instr   (cip_instr),
		.i_cip_j   (cip_j),
		.i_cip_k   (cip_k),
		.i_lip     (lip),
		.i_sj      (s_j_data),
		.o_a_result(a_imm_out),
		.o_s_result(s_imm_out)
	);

	//Address Addition unit
	addr_add aadd (
		.clk     (clk),        //system clock input
		.i_instr (cip_instr),  //7-bit instruction input
		.i_aj    (a_j_data),   //24-bit aj input
		.i_ak    (a_k_data),   //24-bit ak input
		.o_result(a_add_out)
	);  //24-bit output


	//Address Multiply unit
	fast_addr_mult amult (
		.clk     (clk),       //system clock input
		.i_aj    (a_j_data),  //24-bit aj input
		.i_ak    (a_k_data),  //24-bit ak input
		.o_result(a_mul_out)
	);  //24-bit output


	/////////////////////////////////////////////////////////
	//         Memory Controller Functional Unit           //
	/////////////////////////////////////////////////////////

	mem_fu mfu (
		.clk              (clk),
		.rst              (rst),
		.i_cip            (cip),
		.i_cip_vld        (cip_go),
		.i_lip            (lip),
		.i_lip_vld        (lip_vld),
		.i_vector_length  (vector_length),
		.i_vstart         (vfu_start[7]),
		.i_data_base_addr (data_base_addr),
		.i_data_limit_addr(data_limit_addr),
		//interface to V regs
		.i_v0_data        (v_rd_data[63:0]),
		.i_v1_data        (v_rd_data[127:64]),
		.i_v2_data        (v_rd_data[191:128]),
		.i_v3_data        (v_rd_data[255:192]),
		.i_v4_data        (v_rd_data[319:256]),
		.i_v5_data        (v_rd_data[383:320]),
		.i_v6_data        (v_rd_data[447:384]),
		.i_v7_data        (v_rd_data[511:448]),
		.o_v_wr           (vmem_wr),
		.o_v_wr_idx       (vmem_wr_idx),
		.o_v_rd_idx       (vmem_rd_idx),
		//interface to A rf
		.i_a0_data        (a_a0_data),
		.i_ai_data        (a_i_data),
		.i_ak_data        (a_k_data),
		.i_ah_data        (a_h_data),
		.i_a_res_mask     (a_res_mask),
		//interface to s rf
		.i_si_data        (s_i_data),
		.i_s_res_mask     (s_res_mask),
		//interface to B rf
		.o_b_rd_addr      (mem_b_rd_addr),
		.i_b_rd_data      (b_jk_data),
		.o_b_wr_addr      (mem_b_wr_addr),
		.o_b_wr_en        (mem_b_wr_en),
		//interface to T rf
		.o_t_rd_addr      (mem_t_rd_addr),
		.i_t_rd_data      (t_jk_data),
		.o_t_wr_addr      (mem_t_wr_addr),
		.o_t_wr_en        (mem_t_wr_en),
		//memory interface
		.o_mem_busy       (mem_busy),
		.o_range_err      (mem_range_err),
		.o_mem_ce         (mem_ce),
		.o_mem_data       (data_from_mem_to_regs),
		.o_mem_addr       (mem_addr),
		.i_mem_rd_data    (i_data_from_mem),
		.o_mem_wr_data    (data_to_mem),
		.o_mem_wr_en      (mem_wr_en),
		.i_mem_ack        (mem_ack),
		.o_mem_type       (mem_type),
		.o_mem_issue      (mem_issue)
	);


	/////////////////////////////////////////////////////////
	//          DMA "I/O" Controller Logic                 //
	/////////////////////////////////////////////////////////

	assign o_dma_instr     = cip;
	assign o_dma_mon_mode  = mode_mm;
	assign o_dma_instr_vld = cip_vld;
	assign o_dma_ak        = a_k_data;
	assign o_dma_aj        = a_j_data;

	assign dma_type = ((cip[15:6] == 10'o0010) || (cip[15:6] == 10'o0011) || (cip[15:6] == 10'o0012));

	assign dma_issue = dma_type & !(|a_res_mask);

	/////////////////////////////////////////////////////////
	//        InterCPU Communication Logic                 //
	/////////////////////////////////////////////////////////

	//Figure out when it's an instruction targeting the interCPU communication block

	assign intercpu_type =   (XMP != 0) && cip_vld && 
					  (((cip[15:6]==10'b0000001100) && (cip[2:0]==3'b0)) || //0014j0 RT Sj
		((cip[15:9] == 7'b0111010) && (cip[5:0] == 6'b0)) ||  //072i00 Si RT
		((cip[15:9] == 7'b0010110) && (cip[2:0] == 3'h7)) ||  //026ij7 Ai SBj
		((cip[15:9] == 7'b0010111) && (cip[2:0] == 3'h7)) ||  //027ij7 SBj Ai
		((cip[15:9] == 7'b0111010) && (cip[2:0] == 3'h3)) ||  //072ij3 Si STj
		((cip[15:9] == 7'b0111011) && (cip[2:0] == 3'h3)) ||  //073ij3 STj Si
		((cip[15:6] == 10'b0000011100)) ||  //0034jk SMjk 1,TS
		((cip[15:6] == 10'b0000011110)) ||  //0036jk SMjk 0
		((cip[15:6] == 10'b0000011111)) ||  //0037jk SMjk 1
		((cip[15:9] == 7'b0111010) && (cip[5:0] == 6'h02)) ||  //072i02 Si SM
		((cip[15:9] == 7'b0111011) && (cip[5:0] == 6'h02)));  //073i02 SM Si

	//Make I/O assignments							 
	assign o_cln[2:0]           = cln[2:0];
	assign o_intercpu_instr     = cip[15:0];
	//For now, we're going to gate sending instr_vld to the intercpu register block until there are no
	//outstanding writes to A/S registers. This will kill performance, but i don't *think* access to these
	//is terribly performance-critical. This really just underscores the need for a central scoreboard to 
	//check for hazards in a straightforward/high-performance sort of way. The intercpu block can then be dumb,
	//and not worry about hazards.
	assign o_intercpu_instr_vld = cip_vld && !(|a_res_mask) && !(|s_res_mask);
	assign o_intercpu_monmode   = mode_mm;
	assign o_intercpu_sj        = s_j_data;
	assign o_intercpu_si        = s_i_data;
	assign o_intercpu_ai        = a_i_data;

	/////////////////////////////////////////////////////////
	//         Misc. Registers, instruction decoding, etc. //
	/////////////////////////////////////////////////////////


	//Let's increment the real-time clock every cycle
	//Unless it's a 0014x0 instruction, then set the RTC to (Sj)
	//FIXME: This should only work in monitor mode!

	always @(posedge clk)
		real_time_clock <= rst ? 64'b0 :
					  ((cip[15:6]==10'o0014) && (XMP ? (cip_k==3'o0) : 1'b1) && cip_issue && mode_mm) ? s_j_data : (real_time_clock + 64'b1);


	//Programmable Clock
	// 0014j4     PCI Sj      Enter Interrupt Interval register with (Sj)
	always @(posedge clk)
		ii[31:0] <= rst ? 32'b0 : 
					((cip[15:6]==10'o0014) && (cip[2:0]==3'h4) && cip_vld && issue_vld) ? s_j_data[31:0] : ii[31:0];

	// 001405    CCI     Clear the programmable clock interrupt request
	assign clear_prog_clk_int_req = (cip[15:0] == 16'o001405) && cip_vld && issue_vld;

	// 001406    ECI     Enable programmable clock interrupt request
	// 001407    DCI     Disable programmable clock interrupt request
	always @(posedge clk)
		prog_clock_en <= (rst || !XMP) ? 1'b0 : 
								   (cip[15:0]==16'o001406 && cip_vld && issue_vld) ? 1'b1 :
											 (cip[15:0]==16'o001407 && cip_vld && issue_vld) ? 1'b0 :
											 prog_clock_en;

	//ICD - Interrupt Countdown counter:
	//         -> set it when the PCI Sj instruction gets executed
	//         -> If it's enabled, decrement every cycle until it reaches 0, then restore to ii[31:0]
	//         -> Otherwise, just hold steady
	always @(posedge clk)
		icd[31:0] <= rst ? 32'b0 :
					 ((cip[15:6]==10'o0014) && (cip[2:0]==3'h4) && cip_vld && issue_vld) ? s_j_data[31:0] :
							(prog_clock_en && icd[31:0]==32'b0) ? ii[31:0] :
							prog_clock_en ? (icd[31:0] - 32'b1) :
							icd[31:0];

	assign set_prog_clk_int_req = prog_clock_en && (icd[31:0] == 32'b0);


	//Control the vector mask register
	always @(posedge clk)
		if (rst) vector_mask <= 64'hFFFFFFFFFFFFFFFF;
		else if (cip_issue && (cip_instr == 7'o003) && !intercpu_type) vector_mask <= s_j_data;  //(Sj) is 0 when j is 0
		else if (vm_test_out)
			vector_mask <= (tk_out_idx[0]==6'd0) ? {v_test_out,63'b0} : (vector_mask | ({63'b0,v_test_out} << (6'd63 - tk_out_idx[0])));
	//Control the vector length register
	always @(posedge clk)
		if (rst) vector_length <= 7'b1000000;
		else if (!x_swap) begin
			vector_length <= (cip_issue && (cip[15:6]==10'o0020)) ? a_k_data[6:0] : vector_length;   //(Ak) is 1 when k is 0
		end else if (x_load && (x_cnt == 4'b0011)) vector_length <= x_data[39:33];




	/*
   vector_length <= rst ? 7'b1000000 :
                     !x_swap ? 
                     (cip_instr==7'o002) ?  ((cip_k != 3'b0) ? a_k_data[6:0] : 7'b1) :   //for some reason this is supposed to take 3-6 cyles (??)
                        vector_length;
*/


	brancher brnch (
		.clk           (clk),
		.i_issue_vld   (issue_vld),
		.i_cip         (cip),
		.i_cip_vld     (cip_vld),
		.i_lip         (lip),
		.i_lip_vld     (lip_vld),
		.i_a0_neg      (a0_neg),
		.i_a0_pos      (a0_pos),
		.i_a0_zero     (a0_zero),
		.i_a0_nzero    (a0_nzero),
		.i_a0_busy     (a0_busy),
		.i_s0_neg      (s0_neg),
		.i_s0_pos      (s0_pos),
		.i_s0_zero     (s0_zero),
		.i_s0_nzero    (s0_nzero),
		.i_s0_busy     (s0_busy),
		.i_bjk         (b_jk_data),
		.o_branch_type (branch_type),
		.o_branch_issue(branch_issue),
		.o_take_branch (take_branch),
		.o_rtn_jump    (rtn_jump),
		.o_nxt_p       (branch_dest)
	);


	//Program Counter
	//If it's not a branch, increment when we issue the current instruction parcel
	//If it *is* a branch, jump to the appropriate destination
	always @(posedge clk)
		if (rst) p_addr <= 24'b0;
		else if (!x_swap)
			p_addr <= (issue_vld && (nip_in_vld || take_branch)) ? ((take_branch ? branch_dest : (p_addr + 24'b1)) & p_mask) : p_addr;
		else if (x_load && (x_cnt == 4'b0000)) p_addr <= XMP ? {x_data[47:24]} : {2'b0, x_data[45:24]};

	reg alert;
	always @(posedge clk) alert <= rst ? 1'b0 : ((p_addr[23:2] == 22'h207B) || alert);


endmodule
