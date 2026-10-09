// The 6 Mbyte channels of the X-MP: channels 10 to 17 octal, the even
// ones input and the odd ones output (CSM-0111000 pages 2-15 to 2-19, 5-9,
// 5-37 and appendix B).  The reference model is tools/crates/model/src/channel.rs.
//
// A channel moves 16-bit parcels between a device and central memory, four to
// a word, bits 63 to 48 first.  The program enters the limit address CL (0011)
// and then the current address CA (0010), which makes the channel active.
// Channel addresses are absolute.
//
// Input: the device puts a parcel on its data lines and pulses Ready; the
// lines stay as they are until the channel pulses Resume.  Every fourth parcel
// the word is written to memory at (CA) and CA advances; the Resume for that
// parcel follows the write.  At (CA) = (CL) the channel sets its interrupt flag
// and goes inactive.  A Disconnect pulse from the device ends the transfer at
// once; a partly assembled word is stored with zeros in the parcels that did
// not come, and the interrupt flag sets when it is in memory.  A Ready that
// finds the channel inactive is held until it is activated.
//
// Output: the channel reads the word at (CA), advances CA and sends the four
// parcels, each with a Ready pulse, waiting for the device's Resume after each.
// After the last parcel of the word that made (CA) = (CL) it pulses Disconnect,
// sets its interrupt flag and goes inactive.
//
// 0012 clears the flags and stops the channel.  0012j1 also raises the Master
// Clear line of an output channel (0012j0 drops it) and forgets a held Ready
// of an input channel.  033 reads the lowest numbered channel whose interrupt
// flag is set, a current address, or an error flag.  Nothing here sets the
// error flag: there is no parity, and a pulse out of turn is ignored.
//
// I/O Master Clear, a line from the I/O Subsystem, stops every channel and
// clears its addresses and flags (CSM-0111000 page 3-21).
//
// An order (0010 or 0012) can overtake a memory reference of its channel.  A
// reference that still waits for memory is dropped; if it was the store of an
// input word, the Ready of that word's fourth parcel counts as held.  A
// reference under way completes, but it no longer changes the channel: a store
// is followed by its Resume and nothing else, a word read for output is thrown
// away and read again if the channel has been activated anew.

module xmp_channels (
	input wire clk,
	input wire rst,
	input wire i_io_clear, // I/O Master Clear from the I/O Subsystem: every channel stops

	// orders from the program, one clock each
	input wire        i_set_ca,  // 0010: enter CA and activate
	input wire        i_set_cl,  // 0011: enter CL
	input wire        i_clear,   // 0012: clear the flags, stop the channel
	input wire        i_k1,      // 0012j1
	input wire [ 2:0] i_num,     // channel number less 10 octal, for the orders and for 033
	input wire [21:0] i_addr,

	// for 033
	output wire [21:0] o_ca,       // current address of channel i_num
	output wire        o_err,      // its error flag
	output reg  [ 3:0] o_int_num,  // the lowest numbered channel asking, or 0
	output wire        o_int,      // some channel asks for the I/O interrupt

	// memory, absolute addresses, one word a request
	output reg         o_mem_req,
	output reg         o_mem_we,
	output reg  [21:0] o_mem_addr,
	output reg  [63:0] o_mem_wdata,
	input  wire        i_mem_ack,
	input  wire [63:0] i_mem_rdata,

	// devices: element n is the channel pair 10 + 2n and 11 + 2n
	input  wire [ 3:0] i_in_ready,
	input  wire [63:0] i_in_data,
	output reg  [ 3:0] o_in_resume,
	input  wire [ 3:0] i_in_disconnect,
	output reg  [ 3:0] o_out_ready,
	output wire [63:0] o_out_data,
	input  wire [ 3:0] i_out_resume,
	output reg  [ 3:0] o_out_disconnect,
	output reg  [ 3:0] o_out_mc
);

	// state of the eight channels; index = channel number less 10 octal
	reg [21:0] ca[0:7];
	reg [21:0] cl[0:7];
	reg [7:0] active;
	reg [7:0] intr;
	reg [63:0] word[0:7];
	reg [2:0] count[0:7];  // input: parcels assembled; output: parcels left to send
	reg [7:0] want;  // a memory reference is asked for or under way
	reg [7:0] stale;  // the reference under way was overtaken by an order
	reg [3:0] held;  // input: a Ready came while inactive
	reg [3:0] tail;  // input: the store asked for is the part word left by a Disconnect
	reg [3:0] sent;  // output: a parcel is out, waiting for its Resume

	assign o_ca  = ca[i_num];
	assign o_err = 1'b0;
	assign o_int = |intr;

	// (every block has a loop variable of its own: one shared between blocks that
	// are evaluated whenever something changes leaves their order to the simulator)
	integer pi;
	always @(*) begin
		o_int_num = 4'd0;
		for (pi = 7; pi >= 0; pi = pi - 1) if (intr[pi]) o_int_num = {1'b1, pi[2:0]};
	end

	genvar g;
	generate
		for (g = 0; g < 4; g = g + 1) begin : g_data
			assign o_out_data[16*g+:16] = word[2*g+1][63:48];
		end
	endgenerate

	// an input channel takes a parcel: one that comes with Ready, or one that was
	// held.  (Written out channel by channel: Verilator 5.032 lets a loop over the
	// parcel counts go stale.)
	wire [3:0] take;
	wire [3:0] fourth;
	generate
		for (g = 0; g < 4; g = g + 1) begin : g_take
			assign take[g]   = active[2*g] && !want[2*g] && (i_in_ready[g] || held[g]);
			assign fourth[g] = take[g] && (count[2*g] == 3'd3);
		end
	endgenerate

	// one memory reference at a time, the lowest numbered channel first
	reg           busy;
	reg     [2:0] owner;
	reg     [2:0] pick;
	reg           any;
	integer       pk;
	always @(*) begin
		pick = 3'd0;
		any  = 1'b0;
		for (pk = 7; pk >= 0; pk = pk - 1)
		if (want[pk]) begin
			pick = pk[2:0];
			any  = 1'b1;
		end
	end

	wire       set_count = i_set_ca || i_clear;  // the program zeroes the ordered channel's parcel count
	wire       set_ca_owner = i_set_ca && (i_num == owner);  // and gives the channel whose reference ends an address
	wire [1:0] pair = i_num[2:1];  // of the order
	wire       mine = busy && (owner == i_num);  // the ordered channel's reference is under way
	wire       drop = !mine && ((want[i_num] && !tail[pair]) || fourth[pair]);  // an input word's store that waits

	integer n;
	always @(posedge clk) begin
		o_in_resume      <= 4'b0;
		o_out_ready      <= 4'b0;
		o_out_disconnect <= 4'b0;

		if (rst || i_io_clear) begin
			active <= 8'b0;
			intr   <= 8'b0;
			want   <= 8'b0;
			stale  <= 8'b0;
			held   <= 4'b0;
			tail   <= 4'b0;
			sent   <= 4'b0;
			// the Master Clear lines to the devices are 0012's and stay
			if (rst) o_out_mc <= 4'b0;
			busy      <= 1'b0;
			o_mem_req <= 1'b0;
			for (n = 0; n < 8; n = n + 1) begin
				ca[n]    <= 22'd0;
				cl[n]    <= 22'd0;
				count[n] <= 3'd0;
			end
		end else begin
			// ---- memory
			if (!busy && any) begin
				busy        <= 1'b1;
				owner       <= pick;
				o_mem_req   <= 1'b1;
				o_mem_we    <= !pick[0];
				o_mem_addr  <= ca[pick];
				o_mem_wdata <= word[pick];
			end
			if (busy && i_mem_ack) begin
				busy         <= 1'b0;
				o_mem_req    <= 1'b0;
				stale[owner] <= 1'b0;
				if (!owner[0]) begin
					// input: the word is in memory
					want[owner]      <= 1'b0;
					tail[owner[2:1]] <= 1'b0;
					if (!tail[owner[2:1]]) o_in_resume[owner[2:1]] <= 1'b1;
					if (!stale[owner]) begin
						if (tail[owner[2:1]]) intr[owner] <= 1'b1;
						else begin
							if (!set_ca_owner) ca[owner] <= ca[owner] + 22'd1;
							if ((ca[owner] + 22'd1) == cl[owner]) begin
								intr[owner]   <= 1'b1;
								active[owner] <= 1'b0;
							end
						end
					end
				end else if (stale[owner] || !active[owner]) begin
					// output, overtaken: read again if the channel was activated anew
					want[owner] <= active[owner];
				end else begin
					// output: the word is here; send its first parcel
					want[owner] <= 1'b0;
					word[owner] <= i_mem_rdata;
					if (!set_ca_owner) ca[owner] <= ca[owner] + 22'd1;
					if (!(set_count && (i_num == owner))) count[owner] <= 3'd4;
					o_out_ready[owner[2:1]] <= 1'b1;
					sent[owner[2:1]]        <= 1'b1;
				end
			end

			for (n = 0; n < 4; n = n + 1) begin
				// ---- input channel 2n
				if (i_in_ready[n] && !active[2*n]) held[n] <= 1'b1;
				if (take[n]) begin
					held[n] <= 1'b0;
					if (fourth[n]) want[2*n] <= 1'b1;  // Resume follows the write
					else o_in_resume[n] <= 1'b1;
				end
				if (i_in_disconnect[n] && active[2*n]) begin
					active[2*n] <= 1'b0;
					if (count[2*n] == 3'd0) intr[2*n] <= 1'b1;
					else begin
						want[2*n] <= 1'b1;
						tail[n]   <= 1'b1;
					end
				end
				// the word and its count: a Disconnect in the middle of a word comes
				// before a parcel, and the parcels that did not come are zero
				if (i_in_disconnect[n] && active[2*n] && (count[2*n] != 3'd0))
					word[2*n] <= (count[2*n] == 3'd1) ? {word[2*n][15:0], 48'b0} :
								 (count[2*n] == 3'd2) ? {word[2*n][31:0], 32'b0} : {word[2*n][47:0], 16'b0};
				else if (take[n]) word[2*n] <= {word[2*n][47:0], i_in_data[16*n+:16]};
				if (!(set_count && (i_num == {n[1:0], 1'b0}))) begin
					if (i_in_disconnect[n] && active[2*n] && (count[2*n] != 3'd0)) count[2*n] <= 3'd0;
					else if (take[n]) count[2*n] <= fourth[n] ? 3'd0 : count[2*n] + 3'd1;
				end

				// ---- output channel 2n + 1
				if (i_out_resume[n] && sent[n] && active[2*n+1]) begin
					sent[n] <= 1'b0;
					if (!(set_count && (i_num == {n[1:0], 1'b1}))) count[2*n+1] <= count[2*n+1] - 3'd1;
					word[2*n+1] <= {word[2*n+1][47:0], 16'b0};
					if (count[2*n+1] != 3'd1) begin
						o_out_ready[n] <= 1'b1;
						sent[n]        <= 1'b1;
					end else if (ca[2*n+1] == cl[2*n+1]) begin
						o_out_disconnect[n] <= 1'b1;
						intr[2*n+1]         <= 1'b1;
						active[2*n+1]       <= 1'b0;
					end else want[2*n+1] <= 1'b1;
				end
			end

			// ---- the program: these come last and win.  (An address or a count that
			// the program sets is not also written above: each element of a register
			// array is assigned once a clock, which every simulator orders the same.)
			if (i_set_cl) cl[i_num] <= i_addr;
			if (i_clear) begin
				intr[i_num]   <= 1'b0;
				active[i_num] <= 1'b0;
				if (i_num[0]) begin
					o_out_mc[pair] <= i_k1;
				end else if (i_k1) held[pair] <= 1'b0;
			end
			if (i_set_ca) begin
				ca[i_num]     <= i_addr;
				active[i_num] <= 1'b1;
			end
			if (i_set_ca || i_clear) begin
				count[i_num] <= 3'd0;
				if (mine && !i_mem_ack) stale[i_num] <= 1'b1;
				if (i_num[0]) begin
					sent[pair] <= 1'b0;
					if (i_set_ca) want[i_num] <= 1'b1;  // read the first word
					else if (!mine) want[i_num] <= 1'b0;
				end else if (!mine) begin
					want[i_num] <= 1'b0;
					tail[pair]  <= 1'b0;
					if (drop) held[pair] <= 1'b1;
				end
			end
		end
	end

endmodule
