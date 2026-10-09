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
	i_nip_nxt,
	i_word_nxt,
	i_nip_vld,
	o_p_addr,
	o_jump,
	o_jump_addr,
	o_clear_ibufs,
	o_mem_ce,
	o_mem_burst,
	o_mem_seq,
	i_mem_take,
	o_mem_addr,
	i_data_from_mem,
	o_data_to_mem,
	o_mem_wr_en,
	i_mem_ack,
	//Inter-cpu interface
	//Debug interface
	o_debug,
	i_debug_full,
	i_single_step,
	i_mcu_int,
	i_ibuf_busy,
	o_ibuf_hold,
	//6 Mbyte channels
	o_ch_set_ca,
	o_ch_set_cl,
	o_ch_clear,
	o_ch_k1,
	o_ch_num,
	o_ch_addr,
	i_ch_ca,
	i_ch_err,
	i_ch_int_num,
	i_ch_int
);



	// The functional units of a one-processor CRAY X-MP with what the operating
	// system COS needs of it (CSM-0111000): the exchange package with a 24-bit P and
	// separate instruction and data fields, four million words, the cluster number
	// and the shared registers and semaphores, the status register, the operand
	// range and bidirectional memory mode bits.  docs/MACHINE.md lists what it does
	// and does not have.

	`include "cray_types.vh"
	`include "cray_pd.vh"
	//system signals
	input wire clk;
	input wire rst;
	input wire [15:0] i_nip_nxt;
	input wire [63:0] i_word_nxt;
	input wire i_nip_vld;
	output wire [23:0] o_p_addr;
	output wire o_jump;  // a branch is taken this clock
	output wire [23:0] o_jump_addr;  // to this address, as o_p_addr gives one
	output wire o_clear_ibufs;
	//memory interface
	output wire o_mem_ce;
	output wire o_mem_burst;
	output wire o_mem_seq;
	input wire i_mem_take;
	output wire [21:0] o_mem_addr;
	input wire [63:0] i_data_from_mem;
	output reg [63:0] o_data_to_mem;
	output wire o_mem_wr_en;
	input wire i_mem_ack;

	//I/O interface: orders to the 6 Mbyte channels (module xmp_channels),
	//valid in the clock after their instruction issues, and what 033 reads back
	output wire o_ch_set_ca;  // 0010: enter the current address and activate
	output wire o_ch_set_cl;  // 0011: enter the limit address
	output wire o_ch_clear;  // 0012: clear the flags and stop
	output wire o_ch_k1;  // 0012j1
	output wire [2:0] o_ch_num;  // channel number less 10 octal, also for the read
	output wire [21:0] o_ch_addr;
	input wire [21:0] i_ch_ca;  // current address of channel o_ch_num
	input wire i_ch_err;  // its error flag
	input wire [3:0] i_ch_int_num;  // the lowest numbered channel asking for an interrupt, or 0
	input wire i_ch_int;  // some channel asks


	output wire [31:0] o_debug;
	input wire i_debug_full;
	input wire i_single_step;
	input wire i_mcu_int;  // request of the maintenance control unit: the MCU interrupt (flag bit 32)
	input wire i_ibuf_busy;  // an instruction buffer fill is in progress
	output wire o_ibuf_hold;  // do not start a new fill (exchange in progress)
	//Functional unit outputs
	//Functional unit outputs to the A registers; the A result bus says when each is valid
	wire [23:0] a_pop_out;  //24-bit output of scalar population count
	wire [23:0] a_lz_out;  //24-bit output of scalar leading zero count
	wire [23:0] a_mul_out;  //24-bit output of address multiply unit
	wire [23:0] a_add_out;  //24-bit output of address adder
	wire [23:0] a_imm_out;  //24-bit output of immediate values
	//Functional unit outputs to the S registers.  When each is valid, counted from the
	//clock its instruction issues, is told where the S result bus gathers them.
	wire [63:0] s_add_out;  //64-bit output of scalar adder output
	wire [63:0] s_log_out;  //64-bit output of scalar logical unit
	wire [63:0] s_shft_out;  //64-bit output of scalar shift unit
	wire [63:0] s_const_out;  //64-bit output of Scalar constant generator / way to get data from A regs to scalar regs
	wire [63:0] s_imm_out;  //64-bit output of immediate values
	wire [63:0] f_add_out;  //64-bit output of floating point adder unit
	wire [63:0] f_mul_out;  //64-bit output of floating point multiplier unit
	wire [63:0] f_ra_out;  //64-bit output of floating point reciprocal approximation unit
	wire [63:0] f_mul_early;  //the same, a clock before
	wire [63:0] f_ra_early;

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
	wire        s0_pos;
	wire        s0_neg;
	wire        s0_zero;
	wire        s0_nzero;
	wire [ 7:0] s_res_mask;
	wire [ 7:0] s_wait_mask;

	wire [63:0] t_jk_data;
	wire [ 5:0] t_wr_addr;
	wire [ 5:0] t_rd_addr;
	wire [63:0] t_wr_data;
	wire        t_result_en;
	wire        mem_t_wr_en;
	reg         tw_en;  // a 075 issued in the clock before
	reg         bw_en;  // a 025 did


	//address register file signals
	wire [ 2:0] a_ex_addr;
	wire [23:0] a_ex_data;
	wire [23:0] a_j_data;  //address read data
	wire [23:0] a_k_data;
	wire [23:0] a_i_data;
	wire [23:0] a_h_data;
	wire [23:0] a_a0_data;
	wire        a0_pos;
	wire        a0_neg;
	wire        a0_zero;
	wire        a0_nzero;
	wire [ 7:0] a_res_mask;
	wire [ 7:0] a_wait_mask;

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
	wire        mem_issue;  // the memory unit lets the current instruction issue
	wire        vmem_issue;
	wire sc_ready, blk_ready;
	wire sc_pending;  // a scalar reference has issued and is not done
	wire sc_a_v, sc_s_v;  // the word of a scalar load is at the registers in the next clock
	wire [2:0] sc_num;
	wire blk_b, blk_t;  // a block transfer has the B or the T registers
	wire        vmem_unsure;  // a block or vector transfer may still leave the field: what is behind it waits
	wire        mem_ce;
	wire        mem_burst;
	wire        mem_seq;
	wire [63:0] data_to_mem;
	wire        mem_wr_en;
	wire [21:0] mem_addr;
	wire        mem_ack;

	//Cluster number, shared registers, semaphores and status register
	wire [23:0] shr_a;  // (SBj) for 026ij7, while it is in CIP
	wire [63:0] shr_s;  // the result of a 072 form, while it is in CIP
	wire [63:0] status_reg;  // 073i01, while it is in CIP
	wire        ts_wait;  // the test and set in CIP has not looked at its semaphore yet, or found it set
	wire        ts_blocked;  // it found it set
	wire [23:0] ch_a;  // the result of 033, three clocks after it issues

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
	wire fetch_fault;  // the current instruction, or its second parcel, lies outside the field


	reg [ 7:0] xa;
	reg [ 1:0] cln;  //cluster number
	reg        program_state;  //program state bit: stored and loaded only
	reg [23:0] instr_base_addr;
	reg [23:0] instr_limit_addr;
	reg [23:0] data_base_addr;
	reg [23:0] data_limit_addr;

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
	wire [9:0] flags;
	//individual 1-bit flags
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
	wire imm_flags_on;  // the flags interrupt monitor mode enables may set

	//Real Time Clock
	reg [63:0] real_time_clock;

	//Programmable Clock
	reg        pclk_en;  //programmable clock: requests enabled (0014j6, 0014j7)
	reg        pclk_req;  //programmable clock interrupt request
	reg [31:0] icd;  //interrupt countdown counter
	reg [31:0] ii;  //Interrupt interval register

	//******************************************
	//*           Instruction Issue            *
	//*                Logic                   *
	//******************************************

	reg [23:0] p_addr;
	reg [15:0] nip, lip, cip;
	reg nip_vld, lip_vld, cip_vld;

	wire [6:0] cip_instr;
	wire [2:0] cip_i, cip_j, cip_k, cip_h;
	wire issue_vld;
	wire two_parcel_nip, two_parcel_cip;

	//break out the current instruction parcel into fields
	assign cip_instr = cip[15:9];
	assign cip_i     = cip[8:6];
	assign cip_j     = cip[5:3];
	assign cip_k     = cip[2:0];
	assign cip_h     = cip[11:9];

	//A-type scheduler signals: the lanes of results to the A registers
	wire [  AL_N-1:0] a_next_v;
	wire [3*AL_N-1:0] a_next_d;
	wire [8*AL_N-1:0] a_head_hot, a_next_hot;
	wire a_issue;
	wire a_type;
	wire a0_busy;

	//S-type scheduler signals: the lanes of results to the S registers
	wire [SL_N-1:0] s_head_v, s_next_v;
	wire [3*SL_N-1:0] s_next_d;
	wire [8*SL_N-1:0] s_head_hot, s_next_hot;
	wire       s_issue;
	wire       s_type;
	wire [7:0] vreg_swrite;
	wire [7:0] vreg_swrite_raw;
	wire       s0_busy;

	//V-type scheduler signals
	wire [       7:0] vwrite_start;
	wire [       7:0] vread_start;
	wire [       8:0] vfu_start;
	wire [       7:0] vreg_busy;
	wire [       7:0] vreg_reading;
	wire [       7:0] vreg_result;
	wire [       7:0] vreg_coming;
	wire [   7*8-1:0] vreg_come;
	wire [       7:0] vreg_filling;
	wire [   7*8-1:0] vreg_filled;
	wire [       7:0] vreg_step;
	wire [       8:0] vfu_busy;
	wire [(64*8-1):0] v_rd_data;
	wire              v_issue;
	wire              v_type;

	wire exchange_type;


	//single_stepping
	wire instructions_in_flight;
	reg  single_step;
	wire ok_to_run;
	wire cip_go;  // CIP holds an instruction that may start (not displaced by an interrupt)
	//What the issue logic needs to know about the instruction in CIP.  It is decoded
	//while the instruction is in NIP (cray_predecode) and kept in a register that is
	//loaded together with CIP, so that the decision to issue starts from flip-flops.
	wire [PD_W-1:0] pd_nip, pd_none;
	reg  [   PD_W-1:0] pd;
	//the current instruction is one, and its result takes this lane of one clock:
	//for the read ports of the registers (res_regfile.v), kept beside pd
	reg  [SL_LATE-1:0] s_now_lane;
	reg  [AL_LATE-1:0] a_now_lane;
	wire [        7:0] rd_a = pd[PD_RD_A+:8];  // registers the current instruction reads
	wire [        7:0] rd_s = pd[PD_RD_S+:8];
	wire               opnd_busy;  // one of them still has a result on its way
	wire               cip_issue;  // the current instruction issues this clock

	cray_predecode predecode (
		.i_parcel(nip),
		.o_pd    (pd_nip)
	);
	//what the same decode makes of no instruction at all, for when CIP is cleared
	cray_predecode predecode_none (
		.i_parcel(16'b0),
		.o_pd    (pd_none)
	);

	always @(posedge clk) single_step <= i_single_step;

	assign instructions_in_flight = (|a_res_mask) || (|s_res_mask) || (|vreg_busy) || (|vfu_busy) || vm_pending;
	//single stepping: an instruction starts only when nothing is in flight
	assign ok_to_run              = !single_step || !instructions_in_flight;
	assign cip_go                 = cip_vld && !x_take_int && ok_to_run && !opnd_busy && !fetch_fault;
	assign cip_issue              = cip_vld && issue_vld;

	//076 reads a V register that must not be in use; 073 waits for a 175 to finish
	//building the mask, and 003 for a 175 or a merge to leave the logical unit, as
	//another 175 or a merge does itself: the mask is ready for them (VL) + 4 clocks
	//after a 175 has issued and for 073 a clock later (CSM-0111000 page 5-85)
	wire vec_hold = (|(pd[PD_H076+:8] & vreg_busy)) || (pd[PD_HVM] && vm_load) || (pd[PD_H073] && vm_pending) || (pd[PD_H003] && tk_busy[0]) ||
		(pd[PD_H072] && rtc_load) ||
		(pd[PD_HFADD] && tk_busy[4]) || (pd[PD_HFMUL] && fp_mul_busy) || (pd[PD_HFRCP] && tk_busy[5]);
	//The mode instructions 0021 to 0027 and the status register read 073i01 wait for
	//every result on its way and for memory: a floating point error belongs to the
	//modes that held when its instruction issued, and to the status read behind it.
	//What is on its way is taken a clock late, to keep it out of the issue path; the
	//instruction that issued in the clock before is covered by its own flag.
	reg settle_busy, settle_issued;
	always @(posedge clk) begin
		settle_busy   <= instructions_in_flight || !mem_idle;
		settle_issued <= cip_issue;
	end
	wire unsettled = settle_busy || settle_issued;
	wire mode_hold = unsettled && pd[PD_HMODE];
	//A scalar reference waits a clock longer for the register with its address, "Ah
	//reserved or busy previous CP"; what uses the B or the T registers waits for a
	//block transfer of them, "instruction 034 or 035 in process" (CSM-0111000 pages
	//5-20, 5-31, 5-60, 5-64)
	wire mem_hold = (|(pd[PD_AH+:8] & a_res_mask)) || (pd[PD_USE_B] && blk_b) || (pd[PD_USE_T] && blk_t);
	assign opnd_busy = (|(rd_a & a_wait_mask)) || (|(rd_s & s_wait_mask)) || vec_hold || mode_hold || ts_wait || (pd[PD_H074] && tw_en) || (pd[PD_H024] && bw_en) || vmem_unsure || mem_hold;

	/////////////////////////////////////
	//    Logic Analyzer        //
	/////////////////////////////////////
	assign o_debug[31:0] = {xa[7:0], 2'b0, cip_vld && issue_vld, 1'b0, cln, cip_vld, issue_vld, o_p_addr[15:0]};

	//////////////////////////////////////////
	//     Exchange Package Logic           //
	//////////////////////////////////////////
	// The CPU is either running or exchanging.  An exit instruction or an interrupt
	// asks for an exchange; issue stops at once, the parcels behind it are thrown
	// away, everything already issued runs to completion, and then exchange_ctl
	// swaps the registers with the package at XA.

	assign exchange_type = pd[PD_EXCH] && cip_vld && !cip_fault;

	// An interrupt is taken between instructions, as soon as its flag is set: what
	// is the current instruction then does not issue and does nothing, not an exit
	// either.  A memory reference that has issued goes on; the exchange waits for
	// it.  An exit is taken as soon as it is the current instruction.  (When single
	// stepping it waits like any other instruction, so that it issues, and sets its
	// flag, in the clock it is taken.)
	assign x_take_int  = x_run && signal_interrupt;
	assign x_take_exit = x_run && exchange_type && ok_to_run && !vmem_unsure && !x_take_int;
	assign x_request   = x_take_exit || x_take_int;

	assign x_idle = !(|a_res_mask) && !(|s_res_mask) && !(|vreg_busy) && !(|vfu_busy) && !vm_pending && mem_idle && !i_ibuf_busy;

	// The program field.  A parcel whose absolute word address is at or beyond the limit
	// (or beyond the four million words of memory) is not fetched; it travels down the
	// parcel pipeline marked as a fault, and when it would become the current
	// instruction the program range flag sets instead.  The flag cannot set in monitor
	// mode (but in interrupt monitor mode), where the manual does not say what happens;
	// there the fetch is not checked.
	wire [22:0] fetch_limit = (|instr_limit_addr[23:22]) ? 23'h400000 : {1'b0, instr_limit_addr[21:0]};
	// Whether P is outside is kept in a register, formed beside P itself: for the
	// parcel behind this one and for the target of a branch, and one of them taken as
	// P is.  (While an exchange loads P and the limits it follows a clock behind,
	// long before instructions run again.)
	function outside;
		input [21:0] word_addr;  // P without its parcel number
		reg [24:0] word;
		begin
			word    = {3'b0, word_addr} + {1'b0, instr_base_addr};
			outside = imm_flags_on && (word >= {2'b0, fetch_limit});
		end
	endfunction

	wire [23:0] p_behind = p_addr + 24'b1;
	// 005 issues in its third clock as the current instruction, so its target, which
	// comes out of the B registers, is taken from a register a clock old.  The other
	// branches can issue in their first: their target is in CIP and LIP.
	reg  [23:0] branch_dest_r;
	always @(posedge clk) branch_dest_r <= branch_dest;
	wire [23:0] p_target = pd[PD_BR_005] ? branch_dest_r : {cip[7:0], lip};
	reg         p_outside;
	always @(posedge clk)
		p_outside <= (!x_swap && issue_vld && (nip_in_vld || take_branch)) ? (take_branch ? outside(
			p_target[23:2]
		) : outside(
			p_behind[23:2]
		)) : outside(
			p_addr[23:2]
		);
	assign p_oof = p_outside;
	assign nip_in_vld = i_nip_vld || p_oof;
	assign fetch_fault = cip_vld && x_run && !vmem_unsure && !x_take_int && (cip_fault || (two_parcel_cip && nip_fault));

	// No buffer is filled while a scalar reference is on its way: what a program
	// has stored is in memory before the fetch behind it (CSM-0111000 page 2-6)
	assign o_ibuf_hold = !x_run || p_oof || sc_pending;

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
		else if (x_take_exit) p_save <= cip_addr + 24'd1;
		else if (x_take_int) p_save <= cip_vld ? cip_addr : (nip_vld ? (p_addr - 24'd1) : p_addr);

	//look up the appropriate A and S reg values to store them during the exchange sequence
	assign a_ex_addr = x_cnt[2:0];
	assign s_ex_addr = x_cnt[2:0];

	//The instruction buffers always follow the program counter
	assign o_p_addr    = p_addr + {instr_base_addr[21:0], 2'b0};
	//and are told of a branch in the clock it is taken, with where it goes
	assign o_jump      = !x_swap && issue_vld && take_branch;
	assign o_jump_addr = p_target + {instr_base_addr[21:0], 2'b0};

	// Outgoing package word (CSM-0111000 figure 3-3):
	//   word 0  P [47:24]                                             A0 [23:0]
	//   word 1  IBA [47:29]  WS FPS BDM - IMM [28:24]                 A1
	//   word 2  ILA [47:29]  IOR ICM IFP IUM MM [28:24]               A2
	//   word 3  DL [48]  XA [47:40]  VL [39:33]  F [32:24]            A3
	//   word 4  DBA [47:29]  PS [28]  CLN [25:24]                     A4
	//   word 5  DLA [47:29]                                           A5
	//   words 6-7  A6, A7,  words 8-15  S0-S7
	// VNU and ESVL are bit 63 of words 2 and 3.  The processor number, the memory
	// error fields and EAM are stored as 0.
	reg [63:0] x_word;
	always @* begin
		case (x_cnt)
			4'b0000: x_word = {16'b0, p_save, a_ex_data};
			4'b0001: x_word = {16'b0, instr_base_addr[23:5], mode_ws, mode_fps, mode_bdm, 1'b0, mode_imm, a_ex_data};
			4'b0010:
			x_word = {
				mode_vnu, 15'b0, instr_limit_addr[23:5], mode_ior, mode_icm, mode_ifp, mode_ium, mode_mm, a_ex_data
			};
			4'b0011: x_word = {mode_esvl, 14'b0, flag_dl, xa, vector_length, flags[8:0], a_ex_data};
			4'b0100: x_word = {16'b0, data_base_addr[23:5], program_state, 2'b0, cln, a_ex_data};
			4'b0101: x_word = {16'b0, data_limit_addr[23:5], 5'b0, a_ex_data};
			4'b0110: x_word = {40'b0, a_ex_data};
			4'b0111: x_word = {40'b0, a_ex_data};
			default: x_word = s_ex_data;
		endcase
	end

	always @* o_data_to_mem = x_swap ? x_word : data_to_mem;

	//The data memory port belongs to the exchange sequence while it swaps
	assign o_mem_wr_en = x_swap ? x_mem_we : mem_wr_en;
	assign o_mem_ce    = x_swap ? x_mem_req : mem_ce;
	assign o_mem_burst = !x_swap && mem_burst;
	assign o_mem_seq   = !x_swap && mem_seq;
	assign o_mem_addr  = x_swap ? x_mem_addr : mem_addr;
	assign mem_ack     = i_mem_ack && !x_swap;

	//Set up the instr/data base and limit registers: bits 16-34, words 1,2 and 4,5,
	//in units of 32 words
	always @(posedge clk)
		if (rst) instr_base_addr <= 24'b0;
		else if (x_load && (x_cnt == 4'b0001)) instr_base_addr <= {x_data[47:29], 5'b0};

	always @(posedge clk)
		if (rst) instr_limit_addr <= 24'hFFFFFF;
		else if (x_load && (x_cnt == 4'b0010)) instr_limit_addr <= {x_data[47:29], 5'b0};

	always @(posedge clk)
		if (rst) data_base_addr <= 24'b0;
		else if (x_load && (x_cnt == 4'b0100)) data_base_addr <= {x_data[47:29], 5'b0};

	always @(posedge clk)
		if (rst) data_limit_addr <= 24'hFFFFFF;
		else if (x_load && (x_cnt == 4'b0101)) data_limit_addr <= {x_data[47:29], 5'b0};


	//and exchange address
	always @(posedge clk)
		if (rst) xa <= 8'h0;
		else if (x_load && (x_cnt == 4'b0011)) xa <= x_data[47:40];
		else if (!x_swap)
			xa <= ((cip[15:6]==10'o0013) && (cip[2:0]==3'b0) && cip_issue && mode_mm) ? a_j_data[11:4] : xa;   //(Aj) is 0 when j is 0

	//Set the mode bits - Pg 3-9 of CSM-0111000
	//0021 and 0022 switch the floating point interrupt mode, 0023 and 0024 the operand
	//range interrupt mode and 0026 and 0025 the bidirectional memory mode, which is
	//only carried.  They wait for results on their way (mode_hold).
	wire mode_set = cip_issue && (cip[15:9] == 7'o002);
	always @(posedge clk)
		if (rst) begin
			mode_bdm <= 1'b0;
			mode_imm <= 1'b0;
			mode_ior <= 1'b0;
			mode_icm <= 1'b0;
			mode_ifp <= 1'b0;
			mode_ium <= 1'b0;
			mode_mm  <= 1'b0;
		end else if (x_load && (x_cnt == 4'b0001)) begin
			mode_bdm <= x_data[26];
			mode_imm <= x_data[24];
		end else if (x_load && (x_cnt == 4'b0010)) begin
			mode_ior <= x_data[28];
			mode_icm <= x_data[27];
			mode_ifp <= x_data[26];
			mode_ium <= x_data[25];
			mode_mm  <= x_data[24];
		end else if (mode_set)
			case (cip[8:6])
				3'd1:    mode_ifp <= 1'b1;
				3'd2:    mode_ifp <= 1'b0;
				3'd3:    mode_ior <= 1'b1;
				3'd4:    mode_ior <= 1'b0;
				3'd5:    mode_bdm <= 1'b0;
				3'd6:    mode_bdm <= 1'b1;
				default: ;
			endcase

	//Vector not used: set by the package, cleared by the first 076, 077 or 140 to 177
	//that issues, so the system can tell that the V registers of a program need not
	//be kept (CSM-0111000 page 3-10).  Enable second vector logical: the package says
	//whether 140 to 145 may use the second unit.
	reg mode_vnu, mode_esvl;
	always @(posedge clk)
		if (rst) begin
			mode_vnu  <= 1'b0;
			mode_esvl <= 1'b0;
		end else if (x_load && (x_cnt == 4'b0010)) mode_vnu <= x_data[63];
		else if (x_load && (x_cnt == 4'b0011)) mode_esvl <= x_data[63];
		else if (cip_issue && (pd[PD_VTYPE] || (cip[15:10] == 6'o37))) mode_vnu <= 1'b0;

	//The status bits of word 1.  FPS: a floating point error has occurred, whatever
	//the interrupt mode; cleared by 0021 and 0022.  WS: the exchange found a test and
	//set waiting in CIP; it is not loaded from a package.
	always @(posedge clk)
		if (rst) begin
			mode_fps <= 1'b0;
			mode_ws  <= 1'b0;
		end else if (x_load && (x_cnt == 4'b0001)) begin
			mode_fps <= x_data[27];
			mode_ws  <= 1'b0;
		end else begin
			if (mode_set && ((cip[8:6] == 3'd1) || (cip[8:6] == 3'd2))) mode_fps <= 1'b0;
			else if (fp_range_err) mode_fps <= 1'b1;
			if (x_request && ts_blocked) mode_ws <= 1'b1;
		end

	always @(posedge clk)
		if (rst) program_state <= 1'b0;
		else if (x_load && (x_cnt == 4'b0100)) program_state <= x_data[28];



	//Now configure all of the flag bits
	always @(posedge clk)
		if (rst) begin
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
			flag_dl  <= x_data[48];  //bit 15
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
			//A flag sets outside monitor mode only; in interrupt monitor mode the deadlock,
			//floating point, operand range, program range and error exit flags set in
			//monitor mode too.  A flag that cannot set keeps what its package gave it.
			//Deadlock - set when all CPUs in a cluster are holding issue on a test & set instr.
			//With one CPU that is whenever a test and set finds its semaphore set.
			if (imm_flags_on && ts_blocked) flag_dl <= 1'b1;
			//Programmable clock interrupt - set while the clock's request is set.  The
			//request is made when the interrupt countdown counter reaches 0 and stays
			//until 0014j5 clears it, so one made in monitor mode is taken on leaving it.
			if (!mode_mm && pclk_req) flag_pci <= 1'b1;
			//MCU interrupt - set while the maintenance control unit (here the console)
			//holds its request
			if (!mode_mm && i_mcu_int) flag_mcu <= 1'b1;
			//Floating Point Error - set when the floating point range error occurs in any of
			//the floating-point functional units and the enable floating-point interrupt flag is set.
			if (imm_flags_on && mode_ifp && fp_range_err) flag_fpe <= 1'b1;
			//Operand Range Error - set when the data reference is made outside the boundaries of
			//the data base address and data limit address registers, and the Enable Operand Range
			//Interrupt flag (the mode bit IOR) is set.
			if (imm_flags_on && mem_range_err && mode_ior) flag_ore <= 1'b1;
			//Program Range Error - set when an instruction fetch is made outside the boundaries of
			//the Instruction Base Address and Instruction Limit Address registers.
			if (imm_flags_on && fetch_fault) flag_pre <= 1'b1;
			//Memory Error - set when a correctable or uncorrectable memory error occurs and the
			//corresponding enable memory error mode bit is set in the M register.  There are
			//no memory errors here: the flag is what a package brought.
			//I/O Interrupt flag - set while a 6 Mbyte channel holds its interrupt request
			if (!mode_mm && i_ch_int) flag_ioi <= 1'b1;
			//Error Exit - set by an error exit instruction (000)
			if (imm_flags_on && (cip[15:9] == 7'o000) && cip_vld && issue_vld) flag_eex <= 1'b1;
			//Normal Exit - set by a normal exit instruction (004)
			if (!mode_mm && (cip[15:9] == 7'o004) && cip_vld && issue_vld) flag_nex <= 1'b1;
		end

	//Interrupt Monitor Mode "enables all interrupts in monitor mode except PC, MCU,
	//I/O" (CSM-0111000 page 3-9) "and normal exit" (HR-0097 page 3-9): the flags that
	//set, and ask for the exchange, in monitor mode as well when the bit is set
	assign imm_flags_on = !mode_mm || mode_imm;

	//Except for the ME flag, if the program is in monitor mode
	//and the conditions for setting an F register are present, the
	//flag remains cleared and no exchange sequence is initiated
	//(page 3-12 of the X-MP/1 system programmer reference manual).

	assign flags[9:0] = {
		flag_dl, flag_pci, flag_mcu, flag_fpe, flag_ore, flag_pre, flag_me, flag_ioi, flag_eex, flag_nex
	};

	//Ask for the exchange when a flag is set: any flag outside monitor mode, the ones
	//interrupt monitor mode enables in it, and the memory error flag in any mode
	assign signal_interrupt = x_run && (flag_me || (|flags[9:0] && !mode_mm) ||
		(mode_imm && (flag_dl || flag_fpe || flag_ore || flag_pre || flag_eex)));

	//Cluster number: from the package, or by 0014j3 in monitor mode
	always @(posedge clk)
		if (rst) cln <= 2'b0;
		else if (x_load && (x_cnt == 4'b0100)) cln <= x_data[25:24];
		else if (cip_issue && mode_mm && (cip[15:6] == 10'o0014) && (cip[2:0] == 3'd3)) cln <= cip[4:3];

	//1) accept the incoming data from the instruction buffers
	always @(posedge clk)
		if (rst || x_request || x_done) begin
			nip_fault  <= 1'b0;
			cip_fault  <= 1'b0;
			nip        <= 16'b0;
			cip        <= 16'b0;
			lip        <= 16'b0;
			nip_vld    <= 1'b0;
			cip_vld    <= 1'b0;
			lip_vld    <= 1'b0;
			pd         <= pd_none;
			s_now_lane <= {SL_LATE{1'b0}};
			a_now_lane <= {AL_LATE{1'b0}};
		end else if (nip_in_vld && issue_vld) begin
			nip_fault <= p_oof;
			cip_fault <= nip_fault && nip_vld && !take_branch;
			cip_addr <= p_addr - 24'd1;  // NIP holds the parcel before the fetch pointer
			nip <= i_nip_nxt;
			lip <= i_nip_nxt;
			cip <= nip;
			pd <= pd_nip;
			lip_vld <= take_branch ? 1'b0 : two_parcel_nip; //1'b1; //nip_vld;   //Only set it if you have a two_parcel_nip, and you haven't just branched
			nip_vld <= (two_parcel_nip || take_branch) ? 1'b0 : 1'b1;            //Set it if you didn't branch and the current cycle holds a one parcel nip
			cip_vld <= take_branch ? 1'b0 : nip_vld;
			s_now_lane <= {SL_LATE{nip_vld && !take_branch}} & SL_ONE & pd_nip[PD_S_LANE+:SL_LATE];
			a_now_lane <= {AL_LATE{nip_vld && !take_branch}} & AL_ONE & pd_nip[PD_A_LANE+:AL_LATE];
		end  //To catch the case where instruction issues during an I-cache miss.
			 //Set cip_vld=0, preserve everything else.
		else if (issue_vld) begin
			nip        <= nip;
			lip        <= lip;
			cip        <= 16'b0;
			pd         <= pd_none;
			lip_vld    <= 1'b0;  //lip_vld;
			nip_vld    <= take_branch ? 1'b0 : nip_vld;  //nip_vld;
			cip_vld    <= 1'b0;
			s_now_lane <= {SL_LATE{1'b0}};
			a_now_lane <= {AL_LATE{1'b0}};
		end

	//Two parcel instructions
	assign two_parcel_nip = nip_vld && !nip_fault && ((nip[15:10] == 6'b000011) ||  //006-007
		(nip[15:12] == 4'b0001) ||  //010-017
		(nip[15:10] == 6'b001000) ||  //020-021
		(nip[15:10] == 6'b010000) ||  //040-041
		(nip[15:14] == 2'b10));  //100-137


	assign two_parcel_cip = cip_vld && !cip_fault && pd[PD_TWO];

	//Track S-type related reservations, destination data and if we can issue or not
	s_scheduler ssched (
		.clk          (clk),
		.rst          (rst),
		.i_cip_vld    (cip_vld),
		.i_issue_vld  (issue_vld),
		.i_type       (pd[PD_S_TYPE]),
		.i_lane       (pd[PD_S_LANE+:SL_N]),
		.i_dest       (pd[PD_S_DEST+:8]),
		.i_dnum       (pd[PD_S_DNUM+:3]),
		.i_cmask      (pd[PD_S_CMASK+:8]),
		.i_077        (pd[PD_S_077]),
		.i_vw         (pd[PD_S_VW+:8]),
		.i_mem_v      (sc_s_v),
		.i_mem_d      (sc_num),
		.o_s_issue    (s_issue),
		.o_s_type     (s_type),
		.i_vreg_busy  (vreg_busy),
		.o_vreg_write (vreg_swrite_raw),
		.o_head_v     (s_head_v),
		.o_head_hot   (s_head_hot),
		.o_next_v     (s_next_v),
		.o_next_d     (s_next_d),
		.o_next_hot   (s_next_hot),
		.o_s0_busy    (s0_busy),
		.o_s_res_mask (s_res_mask),
		.o_s_wait_mask(s_wait_mask)
	);

	//////////////////////////////////////////////////
	//           Results to the S registers         //
	//////////////////////////////////////////////////
	//Every unit that delivers to the S registers has a way of its own into them (a
	//lane, res_lanes.v), so results of different units that are due in the same
	//clock all arrive in it.  What each lane delivers, and when its value is there:
	//
	//  in the clock the result is due, where an instruction that issues in that
	//  clock takes it off the lane:
	//    the logical unit's result register; the single shifts'; the floating
	//    point adder's; s_now_q, which took what was there when the instruction
	//    issued (040, 041 immediate; 072 clock or shared register; 073 vector mask
	//    and status register; 074 T register); s_mem_q, a word from memory
	//  a clock before it is due, when it is written to its register, which nobody
	//  reads before the result is due:
	//    071 constant; 056, 057 double shift; 060, 061 sum; 076 element of a V
	//    register; 064 to 067 floating product; 070 reciprocal
	wire [4:0] cip_src = pd[PD_S_SRC+:5];

	//076: the element is out of its V register two clocks after the instruction issues
	reg [2:0] v_elem_j1, v_elem_j2;
	reg [63:0] v_elem_q;
	always @(posedge clk) begin
		v_elem_j1 <= cip_j;
		v_elem_j2 <= v_elem_j1;
		v_elem_q  <= vreg_sel(v_rd_data, v_elem_j2);
	end

	reg [63:0] s_now_q, s_mem_q;
	always @(posedge clk) begin
		s_now_q <= ({64{(cip_src == SBUS_IMM) || (cip_src == SBUS_COMP_IMM)}} & s_imm_out) | ({64{cip_src == SBUS_V_MASK}} & vector_mask) |
			({64{cip_src == SBUS_HI_SR}} & status_reg) | ({64{cip_src == SBUS_T_BUS}} & t_jk_data) | ({64{cip_src == SBUS_INTERCPU}} & shr_s);
		s_mem_q <= data_from_mem_to_regs;
	end

	wire [64*SL_N-1:0] s_lane_data;
	assign s_lane_data[64*SL_LOG+:64]   = s_log_out;
	assign s_lane_data[64*SL_NOW+:64]   = s_now_q;
	assign s_lane_data[64*SL_SHIFT+:64] = s_shft_out;
	assign s_lane_data[64*SL_FADD+:64]  = f_add_out;
	assign s_lane_data[64*SL_MEM+:64]   = s_mem_q;
	assign s_lane_data[64*SL_CONST+:64] = s_const_out;
	assign s_lane_data[64*SL_SH2+:64]   = s_shft_out;
	assign s_lane_data[64*SL_ADD+:64]   = s_add_out;
	assign s_lane_data[64*SL_VEL+:64]   = v_elem_q;
	assign s_lane_data[64*SL_FMUL+:64]  = f_mul_early;
	assign s_lane_data[64*SL_FRA+:64]   = f_ra_early;
	//a lane writes at the head, or from stage 1 if it has its value a clock early
	wire [8*SL_N-1:0] s_lane_wr = {s_next_hot[8*SL_N-1:8*SL_LATE], s_head_hot[8*SL_LATE-1:0]};


	//Track A-type related reservations, destination data and if we can issue or not
	a_scheduler asched (
		.clk          (clk),
		.rst          (rst),
		.i_cip_vld    (cip_vld),
		.i_issue_vld  (issue_vld),
		.i_type       (pd[PD_A_TYPE]),
		.i_lane       (pd[PD_A_LANE+:AL_N]),
		.i_dest       (pd[PD_A_DEST+:8]),
		.i_dnum       (pd[PD_A_DNUM+:3]),
		.i_cmask      (pd[PD_A_CMASK+:8]),
		.i_025        (pd[PD_A_025]),
		.i_sconf      (pd[PD_A_SCONF+:8]),
		.i_s_wait_mask(s_wait_mask),
		.i_mem_v      (sc_a_v),
		.i_mem_d      (sc_num),
		.o_a_issue    (a_issue),
		.o_a_type     (a_type),
		.o_head_v     (),
		.o_head_hot   (a_head_hot),
		.o_next_v     (a_next_v),
		.o_next_d     (a_next_d),
		.o_next_hot   (a_next_hot),
		.o_a0_busy    (a0_busy),
		.o_a_res_mask (a_res_mask),
		.o_a_wait_mask(a_wait_mask)
	);


	//////////////////////////////////////////////////
	//           Results to the A registers         //
	//////////////////////////////////////////////////
	//As for the S registers.  In the clock the result is due: the address adder's
	//result register, the leading zero count's and the population count's; a_now_q,
	//which took what was there when the instruction issued (020 to 022 immediate;
	//023 (Sj); 024 B register; 026ij7 shared register); a_mem_q, a word from memory.
	//A clock before: 032 product; 033 channel.
	wire [3:0] cip_asrc = pd[PD_A_SRC+:4];
	wire a_now_imm = (cip_asrc == ABUS_IMM) || (cip_asrc == ABUS_COMP_IMM) || (cip_asrc == ABUS_SIMM) || (cip_asrc == ABUS_S_BUS);

	reg [23:0] a_now_q, a_mem_q;
	always @(posedge clk) begin
		a_now_q <= ({24{a_now_imm}} & a_imm_out) | ({24{cip_asrc == ABUS_B_BUS}} & b_jk_data) | ({24{cip_asrc == ABUS_INTERCPU}} & shr_a);
		a_mem_q <= data_from_mem_to_regs[23:0];
	end

	wire [24*AL_N-1:0] a_lane_data;
	assign a_lane_data[24*AL_NOW+:24] = a_now_q;
	assign a_lane_data[24*AL_ADD+:24] = a_add_out;
	assign a_lane_data[24*AL_LZ+:24]  = a_lz_out;
	assign a_lane_data[24*AL_POP+:24] = a_pop_out;
	assign a_lane_data[24*AL_MEM+:24] = a_mem_q;
	assign a_lane_data[24*AL_MUL+:24] = a_mul_out;
	assign a_lane_data[24*AL_CH+:24]  = ch_a;
	wire [8*AL_N-1:0] a_lane_wr = {a_next_hot[8*AL_N-1:8*AL_LATE], a_head_hot[8*AL_LATE-1:0]};

	//Track V-type instructions
	v_scheduler vsched (
		.i_cip_vld     (cip_go),
		.i_v_type      (v_type),
		.i_vi          (pd[PD_V_I+:8]),
		.i_vj          (pd[PD_V_J+:8]),
		.i_vk          (pd[PD_V_K+:8]),
		.i_fu          (v_fu),
		.o_vwrite_start(vwrite_start),
		.o_vread_start (vread_start),
		.o_vfu_start   (vfu_start),
		.o_v_issue     (v_issue),
		.i_vreg_busy   (vreg_busy),
		.i_vreg_reading(vreg_reading),
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
	//the reciprocal and the population count are both "174 in process": one at a time
	assign vfu_busy[5] = fp_ra_busy | vpop_busy;
	assign vfu_busy[6] = fp_ra_busy | vpop_busy;
	assign vfu_busy[7] = mem_busy;
	assign vfu_busy[8] = fp_mul_busy;
	//The second vector logical unit (CSM-0111000 page 4-18).  It is in the floating
	//point multiply unit, with which it shares the way in and the way out: one is
	//busy when the other is.  A 140 to 145 goes there when the package enables it
	//(ESVL) and it is free, and to the full unit otherwise; the merges and 175 can
	//only use the full unit.
	wire       svl_take = pd[PD_SVL] && mode_esvl && !fp_mul_busy;
	wire [8:0] v_fu = svl_take ? 9'b1_0000_0000 : {1'b0, pd[PD_V_FU+:8]};
	assign mem_idle = !mem_busy;
	assign v_type   = pd[PD_VTYPE];
	assign mem_type = pd[PD_MTYPE];

	//check if it's free to issue

	//A memory instruction issues when the memory unit can take it: a scalar
	//reference into its queue, a block transfer to start.  A vector transfer starts
	//by the V scheduler's word (v_issue); mem_issue is then for an instruction whose
	//transfer has started and that has not issued.
	assign mem_issue = pd[PD_SCREF] ? (sc_ready && lip_vld) : pd[PD_BLOCK] ? blk_ready : vmem_issue;

	assign issue_vld = (
						 (s_issue && s_type && mem_issue && mem_type) ||
						 (s_issue && s_type && !mem_type) ||
						 (a_issue && a_type && mem_issue && mem_type) ||
						 (a_issue && a_type && !mem_type) ||
						 (v_type && mem_type && (v_issue || mem_issue)) ||
						 (v_issue && v_type && !mem_type) ||
						 (branch_issue && branch_type) ||
						 (mem_issue && mem_type && !s_type && !a_type) ||
						 exchange_type ||
						 !(s_type || a_type || v_type || branch_type || mem_type || exchange_type) ||
						 !cip_vld) && (ok_to_run || (v_type && mem_type && mem_issue)) && x_run && !x_take_int && !(cip_vld && opnd_busy) && !fetch_fault;



	//////////////////////////////////////////////////
	//           Register Files                     //
	//////////////////////////////////////////////////

	//The vector registers and the bookkeeping for vector operations in progress.
	//
	//A vector instruction issues once (v_issue).  From then on its functional unit's
	//tracker (v_optrack) says in which clocks operands are at the unit and results
	//come out, and each V register follows its own read or write role.
	//
	//Chaining (CSM-0111000 page 4-12).  An operand register may be one that an
	//earlier instruction is still to fill.  The operation then runs as the data
	//becomes available: its tracker asks for one element after the other, and an
	//element is taken once it is in every operand register that was receiving a
	//result when the instruction issued.  A register says how far its result has
	//come two clocks late (vreg_filling, vreg_filled), so an element is at the
	//unit that takes it as an operand four clocks after it arrived at its
	//register: as long as an element takes from the issue of an instruction.  An
	//instruction that issues by the time element 0 arrives so follows the result
	//stream with nothing lost (full chaining); one that issues later runs behind
	//it (partial chaining).

	//Number of elements a vector instruction processes: VL of 0 means 64 (manual 4-10)
	wire [6:0] vl_count = (vector_length[5:0] == 6'd0) ? 7'd64 : {1'b0, vector_length[5:0]};

	//units with a tracker: 0 logical, 1 shift, 2 integer add, 3 FP multiply, 4 FP add,
	//5 reciprocal, 6 second logical, 7 population count
	localparam NTK    = 8;
	localparam TK_SVL = 6, TK_POP = 7;
	wire [NTK-1:0] tk_busy;
	wire [   15:0] tk_instr     [0:NTK-1];
	wire [   63:0] tk_sj        [0:NTK-1];
	wire [   23:0] tk_ak        [0:NTK-1];
	wire [    5:0] tk_ask       [0:NTK-1];
	wire [NTK-1:0] tk_ask_last;
	wire [NTK-1:0] tk_chain_j;
	wire [NTK-1:0] tk_chain_k;
	wire [NTK-1:0] tk_ok;
	wire [    7:0] tk_step      [0:NTK-1];
	wire [NTK-1:0] tk_in_valid;
	wire [    5:0] tk_in_idx    [0:NTK-1];
	wire [NTK-1:0] tk_in_first;
	wire [NTK-1:0] tk_in_last;
	wire [NTK-1:0] tk_out_valid;
	wire [    5:0] tk_out_idx   [0:NTK-1];
	wire [NTK-1:0] tk_out_last;
	wire [    2:0] tk_out_dest  [0:NTK-1];
	wire [NTK-1:0] tk_out_wr_v;
	wire [   63:0] fu_out       [0:NTK-1];

	assign fu_out[0]      = v_log_out;
	assign fu_out[1]      = v_shft_out;
	assign fu_out[2]      = v_add_out;
	assign fu_out[3]      = f_mul_out;
	assign fu_out[4]      = f_add_out;
	assign fu_out[5]      = f_ra_out;
	assign fu_out[TK_SVL] = v_log2_out;
	assign fu_out[TK_POP] = v_pop_out;

	genvar gu;
	generate
		for (gu = 0; gu < NTK; gu = gu + 1) begin : g_track
			v_optrack #(
				.L((gu == 0) ? 2 : (gu == 1) ? 4 : (gu == 2) ? 3 : (gu == 3) ? 7 : (gu == 4) ? 6 : (gu == 5) ? 14 : (gu == TK_SVL) ? 4 : 6)
			) track (
				.clk(clk),
				.rst(rst),
				.i_start((gu == TK_SVL) ? vfu_start[8] : (gu == TK_POP) ? vfu_start[6] : vfu_start[gu]),
				.i_len(vl_count),
				.i_cip(cip),
				.i_sj(s_j_data),
				.i_ak(a_k_data),
				.i_wr_v((|pd[PD_V_I+:8]) && (cip_instr != 7'o175)),  //a compress index writes Vi by itself
				.i_short((gu == 1) && (cip[10:9] != 2'd2)),  //the shifts but 152
				.i_vj(pd[PD_V_J+:8]),
				.i_vk(pd[PD_V_K+:8]),
				.i_result(vreg_result),
				.o_busy(tk_busy[gu]),
				.o_instr(tk_instr[gu]),
				.o_sj(tk_sj[gu]),
				.o_ak(tk_ak[gu]),
				.o_ask(tk_ask[gu]),
				.o_ask_last(tk_ask_last[gu]),
				.o_chain_j(tk_chain_j[gu]),
				.o_chain_k(tk_chain_k[gu]),
				.i_ok(tk_ok[gu]),
				.o_step(tk_step[gu]),
				.o_in_valid(tk_in_valid[gu]),
				.o_in_idx(tk_in_idx[gu]),
				.o_in_first(tk_in_first[gu]),
				.o_in_last(tk_in_last[gu]),
				.o_out_valid(tk_out_valid[gu]),
				.o_out_idx(tk_out_idx[gu]),
				.o_out_last(tk_out_last[gu]),
				.o_out_dest(tk_out_dest[gu]),
				.o_out_wr_v(tk_out_wr_v[gu])
			);

			//Is the element the operation asks for in its operand registers?  One that
			//was not receiving a result when the instruction issued has it.
			wire [2:0] rj = tk_instr[gu][5:3];
			wire [2:0] rk = tk_instr[gu][2:0];
			wire       ok_j = !tk_chain_j[gu] || !vreg_filling[rj] || ({1'b0, tk_ask[gu]} < vreg_filled[7*rj+:7]);
			wire       ok_k = !tk_chain_k[gu] || !vreg_filling[rk] || ({1'b0, tk_ask[gu]} < vreg_filled[7*rk+:7]);
			//The double left shift 152 joins an element with the one behind it, which
			//it reads from the register a clock later: that one has to be in by then.
			wire       joins = (gu == 1) && (tk_instr[gu][10:9] == 2'd2) && tk_chain_j[gu] && !tk_ask_last[gu];
			wire [6:0] behind = {1'b0, tk_ask[gu]} + 7'd1;
			wire       ok_behind = !joins || !vreg_coming[rj] || (behind < vreg_come[7*rj+:7]);
			assign tk_ok[gu] = ok_j && ok_k && ok_behind;
		end
	endgenerate

	assign vreg_step = tk_step[0] | tk_step[1] | tk_step[2] | tk_step[3] | tk_step[4] | tk_step[5] | tk_step[6] | tk_step[7];

	//a vector load (176) writes one element per word read; a vector store (177) reads
	//the element the memory unit is about to write
	wire       vmem_wr;  //o_mem_data holds element vmem_wr_idx of a 176
	wire       vmem_wr_last;  //and it is the last
	wire [5:0] vmem_wr_idx;
	wire [5:0] vmem_rd_idx;
	wire [7:0] vmem_reads;  //a vector transfer is reading this V register
	wire [2:0] vmem_num;  //the V register of the vector transfer under way
	wire [2:0] vmem_knum;  //the one with the addresses of a gather or scatter

	//077 writes its element in the clock after it issues.  Nothing can tell: the
	//register is free when a 077 issues, and whatever issues next reads a V
	//register a clock or more after it issues itself.  It keeps the issue logic
	//out of the path to the registers' write ports.
	reg [ 7:0] sw_en;
	reg        vm_load;  // a 003 issued in the clock before: (Sj), in sw_data, goes to the vector mask
	reg        rtc_load;  // a 0014j0 did: it goes to the real-time clock
	reg [ 5:0] sw_idx;
	reg [63:0] sw_data;
	always @(posedge clk) begin
		sw_en    <= rst ? 8'b0 : vreg_swrite;
		vm_load  <= !rst && cip_issue && (cip_instr == 7'o003) && !sem_instr;
		rtc_load <= !rst && (cip[15:6] == 10'o0014) && (cip_k == 3'o0) && cip_issue && mode_mm;
		sw_idx   <= a_k_data[5:0];
		sw_data  <= s_j_data;
	end

	//The compress index of 175ijk with k from 4 to 7 (CSM-0111000 page 5-87): the
	//number of every element that passes the test goes to Vi, one behind the other
	//from element 0, and the elements of Vi behind them stay.  A number is written
	//five clocks after its element was tested, so Vi is ready (VL) + 10 clocks after
	//issue; the reservation of Vi ends with the last element tested, written or not.
	localparam NCI = 4;
	wire              v_test_out;  //the logical unit's test of the element it took in
	reg               vm_test_out;  //that test is of element vm_test_idx of a 175,
	reg               vm_test_last;  //the last one,
	reg     [    5:0] vm_test_idx;
	reg               vm_test_ci;  //of a compress index,
	reg     [    2:0] vm_test_dest;  //for this register
	reg     [NCI-1:0] ci_valid;
	reg     [NCI-1:0] ci_hit;
	reg     [NCI-1:0] ci_last;
	reg     [    5:0] ci_idx                                                                          [0:NCI-1];
	reg     [    2:0] ci_dest                                                                         [0:NCI-1];
	reg     [    5:0] ci_ptr;  //the element of Vi the next number goes to
	reg               ci_wr_valid;
	reg               ci_wr_en;
	reg               ci_wr_last;
	reg     [    5:0] ci_wr_ptr;
	reg     [    5:0] ci_wr_num;
	reg     [    2:0] ci_wr_dest;
	wire    [    5:0] ci_ptr_now = (ci_idx[NCI-1] == 6'd0) ? 6'd0 : ci_ptr;  //element 0 starts a list
	integer           ci;
	always @(posedge clk) begin
		ci_valid[0] <= !rst && vm_test_out && vm_test_ci;
		ci_hit[0]   <= v_test_out;
		ci_last[0]  <= vm_test_last;
		ci_idx[0]   <= vm_test_idx;
		ci_dest[0]  <= vm_test_dest;
		for (ci = 1; ci < NCI; ci = ci + 1) begin
			ci_valid[ci] <= !rst && ci_valid[ci-1];
			ci_hit[ci]   <= ci_hit[ci-1];
			ci_last[ci]  <= ci_last[ci-1];
			ci_idx[ci]   <= ci_idx[ci-1];
			ci_dest[ci]  <= ci_dest[ci-1];
		end
		ci_wr_valid <= !rst && ci_valid[NCI-1];
		ci_wr_en    <= ci_valid[NCI-1] && ci_hit[NCI-1];
		ci_wr_last  <= ci_last[NCI-1];
		ci_wr_ptr   <= ci_ptr_now;
		ci_wr_num   <= ci_idx[NCI-1];
		ci_wr_dest  <= ci_dest[NCI-1];
		if (ci_valid[NCI-1]) ci_ptr <= ci_ptr_now + {5'd0, ci_hit[NCI-1]};
	end

	//A result is a clock on its way from its unit to the V register: with it the
	//register is free (VL) + unit time + 5 clocks after issue, as the X-MP's is
	//(CSM-0111000 section 5).  The population count unit, the last of the units,
	//has that clock in itself.
	localparam NVW = NTK - 1;
	reg     [NVW-1:0] vw_valid;
	reg     [NVW-1:0] vw_last;
	reg     [    5:0] vw_idx   [0:NVW-1];
	reg     [    2:0] vw_dest  [0:NVW-1];
	reg     [   63:0] vw_data  [0:NVW-1];
	integer           w;
	always @(posedge clk)
		for (w = 0; w < NVW; w = w + 1) begin
			vw_valid[w] <= !rst && tk_out_valid[w] && tk_out_wr_v[w];
			vw_last[w]  <= tk_out_last[w];
			vw_idx[w]   <= tk_out_idx[w];
			vw_dest[w]  <= tk_out_dest[w];
			vw_data[w]  <= fu_out[w];
		end
	wire pop_valid = tk_out_valid[NVW] && tk_out_wr_v[NVW];

	genvar gr;
	generate
		for (gr = 0; gr < 8; gr = gr + 1) begin : g_vreg
			//who writes this register now: a functional unit, a vector load, a compress
			//index or a 077; and whether that is the last of a result (wr_last)
			reg            wr_en;
			reg            wr_last;
			reg     [ 5:0] wr_idx;
			reg     [63:0] wr_data;
			integer        u;
			always @* begin
				wr_en   = 1'b0;
				wr_last = 1'b0;
				wr_idx  = sw_idx;
				wr_data = sw_data;
				if (sw_en[gr])  //077: (Sj) to element (Ak), issued in the clock before
					wr_en = 1'b1;
				if (vmem_wr && (vmem_num == gr)) begin
					wr_en   = 1'b1;
					wr_last = vmem_wr_last;
					wr_idx  = vmem_wr_idx;
					wr_data = data_from_mem_to_regs;
				end
				for (u = 0; u < NVW; u = u + 1)
				if (vw_valid[u] && (vw_dest[u] == gr)) begin
					wr_en   = 1'b1;
					wr_last = vw_last[u];
					wr_idx  = vw_idx[u];
					wr_data = vw_data[u];
				end
				if (pop_valid && (tk_out_dest[NVW] == gr)) begin
					wr_en   = 1'b1;
					wr_last = tk_out_last[NVW];
					wr_idx  = tk_out_idx[NVW];
					wr_data = fu_out[NVW];
				end
				if (ci_wr_valid && (ci_wr_dest == gr)) begin
					wr_en   = ci_wr_en;
					wr_last = ci_wr_last;
					wr_idx  = ci_wr_ptr;
					wr_data = {58'b0, ci_wr_num};
				end
			end

			v_regfile vreg (
				.clk       (clk),
				.rst       (rst),
				.i_rd_start(vread_start[gr] && !mem_type),
				.i_rd_step (vreg_step[gr]),
				.i_len     (vl_count),
				.i_elem_idx(a_k_data[5:0]),
				.i_mem_rd  (vmem_reads[gr]),
				.i_mem_idx (vmem_rd_idx),
				.o_rd_data (v_rd_data[64*gr+:64]),
				.i_wr_start(vwrite_start[gr]),
				.i_wr_last (wr_last),
				.i_wr_en   (wr_en),
				.i_wr_idx  (wr_idx),
				.i_wr_data (wr_data),
				.o_busy    (vreg_busy[gr]),
				.o_reading (vreg_reading[gr]),
				.o_result  (vreg_result[gr]),
				.o_coming  (vreg_coming[gr]),
				.o_come    (vreg_come[7*gr+:7]),
				.o_filling (vreg_filling[gr]),
				.o_filled  (vreg_filled[7*gr+:7])
			);
		end
	endgenerate

	//operands for the units, picked by the instruction each tracker holds
	function [63:0] vreg_sel;
		input [511:0] all;
		input [2:0] n;
		begin
			vreg_sel = all[64*n+:64];
		end
	endfunction

	//The S registers.  Port 0 reads Sj, which is nothing for j = 0; port 1 Sk, which
	//for k = 0 is a word with only its sign bit set; port 2 Si.
	wire [63:0] s_j_raw, s_k_raw;
	wire s_r0_neg, s_r0_zero;
	res_regfile #(
		.W   (64),
		.NL  (SL_N),
		.NB  (SL_LATE),
		.ONE (SL_ONE),
		.NP  (3),
		.ZERO(3'b011)
	) s_rf (
		.clk       (clk),
		.rst       (rst),
		.i_data    (s_lane_data),
		.i_wr      (s_lane_wr),
		.i_x_en    (x_load && x_cnt[3]),
		.i_x_addr  (s_ex_addr),
		.i_x_data  (x_data),
		.i_addr    ({cip_i, cip_k, cip_j}),
		.i_addr_nxt({nip[8:6], nip[2:0], nip[5:3]}),
		.i_issue   (issue_vld),
		.i_next_v  (s_next_v[SL_LATE-1:0]),
		.i_next_d  (s_next_d[3*SL_LATE-1:0]),
		.i_now_lane(s_now_lane),
		.i_now_d   (pd[PD_S_DNUM+:3]),
		.o_data    ({s_i_data, s_k_raw, s_j_raw}),
		.i_ex_addr (s_ex_addr),
		.o_ex_data (s_ex_data),
		.o_r0_neg  (s_r0_neg),
		.o_r0_zero (s_r0_zero)
	);
	assign s_j_data = s_j_raw;
	assign s_k_data = {s_k_raw[63] | (cip_k == 3'd0), s_k_raw[62:0]};
	//These signals are used for branching
	assign s0_pos   = !s_r0_neg;
	assign s0_neg   = s_r0_neg;
	assign s0_zero  = s_r0_zero;
	assign s0_nzero = !s_r0_zero;



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

	//075 writes (Si) to Tjk in the clock after it issues, which keeps the issue logic
	//and the way an operand takes out of the write port of a block memory.  A 074
	//right behind it waits that one clock, so that it reads what was written; a block
	//transfer needs longer than that to get to the registers.  036 writes from memory.
	reg [ 5:0] tw_addr;
	reg [63:0] tw_data;
	always @(posedge clk) begin
		tw_en   <= !rst && cip_issue && (cip_instr == 7'o075);
		tw_addr <= {cip_j, cip_k};
		tw_data <= s_i_data;
	end

	assign t_rd_addr   = blk_t ? mem_t_rd_addr : {cip_j, cip_k};
	assign t_wr_addr   = tw_en ? tw_addr : mem_t_wr_addr;
	assign t_wr_data   = tw_en ? tw_data : data_from_mem_to_regs;
	assign t_result_en = mem_t_wr_en || tw_en;

	//The A registers.  Port 0 reads Aj, port 1 Ak and port 3 Ah, each nothing for
	//register 0 (and Ak then is 1); port 2 Ai; port 4 A0.
	wire [23:0] a_k_raw;
	wire a_r0_neg, a_r0_zero;
	res_regfile #(
		.W   (24),
		.NL  (AL_N),
		.NB  (AL_LATE),
		.ONE (AL_ONE),
		.NP  (5),
		.ZERO(5'b01011),
		.LATE(5'b01000)
	) A_rf (
		.clk       (clk),
		.rst       (rst),
		.i_data    (a_lane_data),
		.i_wr      (a_lane_wr),
		.i_x_en    (x_load && !x_cnt[3]),
		.i_x_addr  (a_ex_addr),
		.i_x_data  (x_data[23:0]),
		.i_addr    ({3'd0, cip_h, cip_i, cip_k, cip_j}),
		.i_addr_nxt({3'd0, nip[11:9], nip[8:6], nip[2:0], nip[5:3]}),
		.i_issue   (issue_vld),
		.i_next_v  (a_next_v[AL_LATE-1:0]),
		.i_next_d  (a_next_d[3*AL_LATE-1:0]),
		.i_now_lane(a_now_lane),
		.i_now_d   (pd[PD_A_DNUM+:3]),
		.o_data    ({a_a0_data, a_h_data, a_i_data, a_k_raw, a_j_data}),
		.i_ex_addr (a_ex_addr),
		.o_ex_data (a_ex_data),
		.o_r0_neg  (a_r0_neg),
		.o_r0_zero (a_r0_zero)
	);
	assign a_k_data = {a_k_raw[23:1], a_k_raw[0] | (cip_k == 3'd0)};
	//These signals are used for branching
	assign a0_pos   = !a_r0_neg;
	assign a0_neg   = a_r0_neg;
	assign a0_zero  = a_r0_zero;
	assign a0_nzero = !a_r0_zero;


	//025 writes (Ai) to Bjk, and a return jump writes P to B00, in the clock after the
	//instruction issues, for the same reason.  A 024 right behind a 025 waits that one
	//clock; a branch to (Bjk) and a block transfer need longer than that to get to the
	//registers, and what follows a return jump comes from its target.  034 writes from
	//memory.
	reg        rj_en;
	reg [ 5:0] bw_addr;
	reg [23:0] bw_data;
	reg [23:0] rj_p;
	always @(posedge clk) begin
		bw_en   <= !rst && (cip_instr == 7'o025) && a_issue && cip_issue;
		bw_addr <= {cip_j, cip_k};
		bw_data <= a_i_data;
		rj_en   <= !rst && rtn_jump && cip_issue;
		rj_p    <= p_addr;
	end

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
		.i_cur_p   (rj_p),
		.i_rtn_jump(rj_en)
	);

	//Figure out when and what we should write into the B register file
	assign b_wr_addr  = bw_en ? bw_addr : mem_b_wr_addr;
	assign b_wr_data  = bw_en ? bw_data : data_from_mem_to_regs[23:0];
	assign b_write_en = bw_en || mem_b_wr_en;

	//and figure out what address to read from
	assign b_rd_addr = blk_b ? mem_b_rd_addr : {cip_j, cip_k};

	//////////////////////////////////////////////////
	//           Vector Units                       //
	//////////////////////////////////////////////////
	//Each unit is a plain pipeline.  The first operand is (Sj) for the even
	//instructions and the Vj element for the odd ones; the second is the Vk element.

	//Vector Logical unit (140-147) and the element test of 175
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

	//Second Vector Logical unit (140-145 with ESVL).  Its unit time is four clocks,
	//two more than the full unit's (CSM-0111000 page 4-19).
	wire [63:0] v_log2_raw;
	wire        v_log2_test;
	reg [63:0] v_log2_q, v_log2_out;
	vector_logical vlog2 (
		.clk     (clk),
		.i_op    (tk_instr[TK_SVL][11:9]),
		.i_test  (2'b0),
		.i_a     (tk_instr[TK_SVL][9] ? vreg_sel(v_rd_data, tk_instr[TK_SVL][5:3]) : tk_sj[TK_SVL]),
		.i_b     (vreg_sel(v_rd_data, tk_instr[TK_SVL][2:0])),
		.i_vm_bit(1'b0),
		.o_result(v_log2_raw),
		.o_test  (v_log2_test)
	);
	always @(posedge clk) begin
		v_log2_q   <= v_log2_raw;
		v_log2_out <= v_log2_q;
	end

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

	//Vector Population Count unit (174ij1 counts, 174ij2 their parities)
	wire [63:0] v_pop_out;
	vector_pop vpop (
		.clk     (clk),
		.i_parity(tk_instr[TK_POP][1]),
		.i_d     (vreg_sel(v_rd_data, tk_instr[TK_POP][5:3])),
		.o_result(v_pop_out)
	);

	assign vpop_busy = tk_busy[TK_POP];

	assign vlog_busy   = tk_busy[0];
	assign vshift_busy = tk_busy[1];
	assign vadd_busy   = tk_busy[2];

	//The vector mask instruction 175 builds VM one element at a time.  The test of an
	//element is made in the clock after the logical unit has taken it in, a clock
	//before a result would leave the unit.  Element 0 is mask bit 63; elements not
	//tested are zero.
	reg vm_pending;  //a 175 has issued and its last test is not in yet
	always @(posedge clk) begin
		vm_test_out  <= !rst && tk_in_valid[0] && (tk_instr[0][15:9] == 7'o175);
		vm_test_last <= tk_in_last[0];
		vm_test_idx  <= tk_in_idx[0];
		vm_test_ci   <= tk_instr[0][2];
		vm_test_dest <= tk_instr[0][8:6];
	end
	always @(posedge clk)
		if (rst) vm_pending <= 1'b0;
		else if (vfu_start[0] && (cip_instr == 7'o175)) vm_pending <= 1'b1;
		else if (vm_test_out && vm_test_last) vm_pending <= 1'b0;

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
		.o_range_err(fp_mul_err),
		.o_early    (f_mul_early)
	);

	//Floating Point Reciprocal Approximation unit (070; 174)
	wire fr_vec = tk_in_valid[5];
	fp_recip frecip (
		.clk        (clk),
		.i_a        (fr_vec ? vreg_sel(v_rd_data, tk_instr[5][5:3]) : s_j_data),
		.o_result   (f_ra_out),
		.o_range_err(fp_ra_err),
		.o_early    (f_ra_early)
	);

	assign fp_mul_busy = tk_busy[3] | tk_busy[TK_SVL];
	assign fp_add_busy = tk_busy[4];
	assign fp_ra_busy  = tk_busy[5];

	//A floating point range error is reported when the result is delivered
	wire fp_range_err = (s_head_v[SL_FADD] && fp_add_err) || (s_head_v[SL_FMUL] && fp_mul_err) || (s_head_v[SL_FRA] && fp_ra_err) ||
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
		.clk     (clk),            //system clock input
		.i_parity(cip_k == 3'd1),  //026ij1
		.i_sj    (s_j_data),       //64-bit sj input
		.o_pop   (a_pop_out),      //24-bit outputs
		.o_lz    (a_lz_out)
	);

	//Scalar Shift unit
	scalar_shift sshift (
		.clk     (clk),
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
		.i_instr   (cip_instr),
		.i_cip_i   (cip_i),
		.i_cip_j   (cip_j),
		.i_cip_k   (cip_k),
		.i_lip     (lip),
		.i_sj      (s_j_data),
		.i_vl      (vector_length),
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
		.i_lip            (lip),
		.i_scalar         (pd[PD_SCREF]),
		.i_block          (pd[PD_BLOCK]),
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
		.o_v_last         (vmem_wr_last),
		.o_v_wr_idx       (vmem_wr_idx),
		.o_v_rd_idx       (vmem_rd_idx),
		.i_v_avail        (!vreg_filling[vmem_num] || ({1'b0, vmem_rd_idx} < vreg_filled[7*vmem_num+:7])),
		.i_vk_avail       (!vreg_filling[vmem_knum] || ({1'b0, vmem_rd_idx} < vreg_filled[7*vmem_knum+:7])),
		//interface to A rf
		.i_a0_data        (a_a0_data),
		.i_ai_data        (a_i_data),
		.i_ak_data        (a_k_data),
		.i_ah_data        (a_h_data[21:0]),
		.o_sc_a           (sc_a_v),
		//interface to s rf
		.i_si_data        (s_i_data),
		.o_sc_s           (sc_s_v),
		.o_sc_num         (sc_num),
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
		.o_mem_burst      (mem_burst),
		.o_mem_seq        (mem_seq),
		.i_mem_take       (i_mem_take && !x_swap),
		.o_mem_data       (data_from_mem_to_regs),
		.o_mem_addr       (mem_addr),
		.i_mem_rd_data    (i_data_from_mem),
		.o_mem_wr_data    (data_to_mem),
		.o_mem_wr_en      (mem_wr_en),
		.i_mem_ack        (mem_ack),
		.o_sc_ready       (sc_ready),
		.o_blk_ready      (blk_ready),
		.o_mem_issue      (vmem_issue),
		.i_issue          (cip_issue),
		.o_v_num          (vmem_num),
		.o_vk_num         (vmem_knum),
		.o_v_reads        (vmem_reads),
		.o_blk_b          (blk_b),
		.o_blk_t          (blk_t),
		.o_unsure         (vmem_unsure),
		.o_sc_pending     (sc_pending)
	);


	/////////////////////////////////////////////////////////
	//   Shared registers, semaphores, status              //
	/////////////////////////////////////////////////////////
	//Three clusters, each with eight 24-bit SB registers, eight 64-bit ST registers and
	//32 semaphores (CSM-0111000 pages 2-17, 5-17, 5-32, 5-34, 5-59).  The cluster number
	//picks the set; in cluster 0 a store does nothing and a load gives zero.
	//  026ij7  Ai SBj      027ij7  SBj Ai
	//  072ij3  Si STj      073ij3  STj Si
	//  072i02  Si SM       073i02  SM Si      the semaphores are bits 63 to 32, SM0 first
	//  0034jk  test and set semaphore jk: it cannot issue while the semaphore is set
	//  0036jk  clear semaphore jk          0037jk  set semaphore jk
	//  073i01  Si SR0      the status register
	//A store acts in the clock its instruction issues, and a load takes what the
	//register holds in that clock, so a load right behind a store sees it.
	wire sem_instr = (cip[15:9] == 7'o003) && cip[8] && (cip[8:6] != 3'd5);  //0034, 0036, 0037

	(* ramstyle = "MLAB, no_rw_check" *)reg [23:0] sb[0:31];
	(* ramstyle = "MLAB, no_rw_check" *)reg [63:0] st[0:31];
	//four words of flip-flops: as a block memory they sat far from the S registers
	(* ramstyle = "logic" *)reg [31:0] sm[ 0:3];  //bit 31 is semaphore 0; cluster 0 is never used

	wire        clustered = (cln != 2'd0);
	wire [ 4:0] reg_n = {cln, cip[5:3]};
	wire [31:0] sem_bit = 32'h80000000 >> cip[4:0];  //the semaphore jk names
	wire        is_ts = pd[PD_TS];
	wire [31:0] sem_now = sm[cln];

	//A test and set looks at its semaphore in its first clock as the current
	//instruction and issues, or waits, from the second.  The look goes through
	//the cluster number and the semaphore number, too long a way to stand before
	//the issue of every instruction.  With one CPU nothing changes a semaphore
	//while the instruction waits.
	reg ts_seen, ts_set;
	always @(posedge clk) begin
		ts_seen <= !rst && cip_vld && is_ts && !cip_issue;
		ts_set  <= clustered && (|(sem_now & sem_bit));
	end
	assign ts_wait    = cip_vld && is_ts && clustered && (!ts_seen || ts_set);
	assign ts_blocked = cip_vld && is_ts && ts_seen && ts_set && !vmem_unsure;

	always @(posedge clk) begin
		if (cip_issue && clustered) begin
			if ((cip[15:9] == 7'o027) && (cip[2:0] == 3'd7)) sb[reg_n] <= a_i_data;
			if ((cip[15:9] == 7'o073) && (cip[2:0] == 3'd3)) st[reg_n] <= s_i_data;
			if ((cip[15:9] == 7'o073) && (cip[5:0] == 6'o02)) sm[cln] <= s_i_data[63:32];
			else if (cip[15:6] == 10'o0036) sm[cln] <= sem_now & ~sem_bit;
			else if (is_ts || (cip[15:6] == 10'o0037)) sm[cln] <= sem_now | sem_bit;
		end
	end

	assign shr_a = clustered ? sb[reg_n] : 24'b0;
	//072i00 is still the real-time clock
	assign shr_s = (cip[5:0] == 6'o00) ? real_time_clock : !clustered ? 64'b0 : (cip[2:0] == 3'd3) ? st[reg_n] : {sem_now, 32'b0};
	//clustered, program state, floating point error status and the three mode
	//bits; the cluster number only in monitor mode; ones in the low half
	assign status_reg = {
		clustered,
		5'b0,
		program_state,
		5'b0,
		mode_fps,
		mode_ifp,
		mode_ior,
		mode_bdm,
		14'b0,
		mode_mm ? cln : 2'b0,
		32'hFFFFFFFF
	};

	/////////////////////////////////////////////////////////
	//   Orders to the 6 Mbyte channels, and 033           //
	/////////////////////////////////////////////////////////
	//  0010jk  CA,Aj Ak    0011jk  CL,Aj Ak    0012j0  CI,Aj    0012j1  MC,Aj
	//act in monitor mode when j is not 0 and the low four bits of (Aj) name a channel
	//from 10 to 17 octal (CSM-0111000 pages 5-9 and 5-37).  An order reaches the
	//channels the clock after its instruction issues.  033 reads them in that same
	//later clock, so a 033 right behind a 0012 sees what the 0012 did:
	//  033i0x  Ai CI       033ij0  Ai CA,Aj    033ij1  Ai CE,Aj
	//The result is due four clocks after issue.
	reg ch_set_ca, ch_set_cl, ch_clear, ch_k1;
	reg [ 2:0] ch_num;
	reg [21:0] ch_addr;
	reg ch_rd_int, ch_rd_none, ch_rd_err;
	reg [23:0] ch_rd1, ch_rd2;
	wire ch_order = cip_issue && mode_mm && (cip[15:9] == 7'o001) && (cip_j != 3'd0) && a_j_data[3];

	always @(posedge clk) begin
		ch_set_ca <= !rst && ch_order && (cip[8:6] == 3'd0);
		ch_set_cl <= !rst && ch_order && (cip[8:6] == 3'd1);
		ch_clear <= !rst && ch_order && (cip[8:6] == 3'd2);
		ch_k1 <= (cip_k == 3'd1);
		ch_num <= a_j_data[2:0];
		ch_addr <= a_k_data[21:0];
		ch_rd_int <= (cip_j == 3'd0);
		ch_rd_none <= !a_j_data[3];
		ch_rd_err <= cip_k[0];
		ch_rd1 <= ch_rd_int ? {20'b0, i_ch_int_num} : ch_rd_none ? 24'b0 : ch_rd_err ? {23'b0, i_ch_err} : {2'b0, i_ch_ca};
		ch_rd2 <= ch_rd1;
	end

	assign o_ch_set_ca = ch_set_ca;
	assign o_ch_set_cl = ch_set_cl;
	assign o_ch_clear  = ch_clear;
	assign o_ch_k1     = ch_k1;
	assign o_ch_num    = ch_num;
	assign o_ch_addr   = ch_addr;
	assign ch_a        = ch_rd2;

	/////////////////////////////////////////////////////////
	//         Misc. Registers, instruction decoding, etc. //
	/////////////////////////////////////////////////////////


	//Let's increment the real-time clock every cycle
	//Unless it's a 0014x0 instruction, then set the RTC to (Sj)
	//FIXME: This should only work in monitor mode!

	//The value arrives a clock after its instruction issues, like the element of a
	//077: it keeps the issue logic away from a wide register.  A 072 right behind
	//waits that clock.
	always @(posedge clk) real_time_clock <= rst ? 64'b0 : rtc_load ? sw_data : (real_time_clock + 64'b1);


	//Programmable clock (HR-0004 rev F pages 4-10 and 6-23), monitor mode only:
	// 0014j4    PCI Sj  enter the interrupt interval and the countdown with (Sj)
	// 0014j5    CCI     clear the interrupt request
	// 0014j6    ECI     enable the interrupt request
	// 0014j7    DCI     disable the interrupt request
	//The countdown runs all the time.  At zero it takes the interval again and, if
	//enabled, sets the request, which stays set until 0014j5.  The request raises
	//flag bit 31 outside monitor mode.
	//The instruction acts in the clock after it issues.  No program can tell (the
	//clock cannot be read and its interrupt is not taken in monitor mode), and it
	//keeps the issue logic away from these registers.
	reg        pclk_op;
	reg [ 2:0] pclk_k;
	reg [31:0] pclk_sj;
	always @(posedge clk) begin
		pclk_op <= !rst && (cip[15:6] == 10'o0014) && cip_k[2] && cip_issue && mode_mm;
		pclk_k  <= cip_k;
		pclk_sj <= s_j_data[31:0];
	end

	wire pclk_load = pclk_op && (pclk_k == 3'd4);
	always @(posedge clk)
		if (rst) begin
			ii       <= 32'b0;
			icd      <= 32'b0;
			pclk_en  <= 1'b0;
			pclk_req <= 1'b0;
		end else begin
			if (pclk_load) begin
				ii  <= pclk_sj;
				icd <= pclk_sj;
			end else if (icd == 32'b0) icd <= ii;
			else icd <= icd - 32'b1;

			if (pclk_op && (pclk_k == 3'd5)) pclk_req <= 1'b0;
			if (!pclk_load && (icd == 32'b0) && pclk_en) pclk_req <= 1'b1;

			if (pclk_op && (pclk_k == 3'd6)) pclk_en <= 1'b1;
			if (pclk_op && (pclk_k == 3'd7)) pclk_en <= 1'b0;
		end


	//Control the vector mask register
	always @(posedge clk)
		if (rst) vector_mask <= 64'hFFFFFFFFFFFFFFFF;
		else if (vm_load) vector_mask <= sw_data;  //(Sj), which is 0 when j is 0, a clock after 003 issues
		else if (vm_test_out)
			vector_mask <= (vm_test_idx==6'd0) ? {v_test_out,63'b0} : (vector_mask | ({63'b0,v_test_out} << (6'd63 - vm_test_idx)));
	//Control the vector length register
	always @(posedge clk)
		if (rst) vector_length <= 7'b1000000;
		else if (!x_swap) begin
			//the low six bits of (Ak), which is 1 when k is 0, and "the 7th bit of VL is
			//set if the 6 low-order bits of (Ak) = 0" (CSM-0111000 page 5-14)
			if (cip_issue && (cip[15:6] == 10'o0020)) vector_length <= {(a_k_data[5:0] == 6'b0), a_k_data[5:0]};
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
		.i_type        (pd[PD_BTYPE]),
		.i_005         (pd[PD_BR_005]),
		.i_on_a0       (pd[PD_BR_A0]),
		.i_on_s0       (pd[PD_BR_S0]),
		.i_jump        (pd[PD_BR_JMP]),
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
		.i_b_written   (bw_en || blk_b),
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
			p_addr <= (issue_vld && (nip_in_vld || take_branch)) ? (take_branch ? p_target : p_behind) : p_addr;
		else if (x_load && (x_cnt == 4'b0000)) p_addr <= x_data[47:24];

	reg alert;
	always @(posedge clk) alert <= rst ? 1'b0 : ((p_addr[23:2] == 22'h207B) || alert);


endmodule
