// An Ampex Dialogue 80, the terminal the I/O Subsystem software drives its
// consoles as: 24 lines of 80 characters and a status line, two pages.
//
// This is the terminal's controller: what its firmware does with the
// characters it receives and the keys that are pressed.  It does what the
// terminal of the reference model does (tools/crates/ios/src/screen.rs, which
// was made from the firmware and is checked against it), routine by routine and
// with the same things remembered between them, so that what the firmware does
// by accident comes out here by the same accident.  sim/tb/term_ampex_tb.v and
// sim/harness/ampex_main.cpp check the one against the other.
//
// A character or a key is worked off to its end before the next is taken:
// din_ready and key_ready are low until the screen memory and the status line
// are as the character leaves them and everything it makes the terminal send
// has been taken at dout.  If a character and a key are offered in the same
// clock both are taken, and the character is worked off first.
//
// What the terminal knows:
//   the control characters BEL BS HT LF VT FF CR SUB RS US
//   ESC = row column     cursor address      ESC ?        the terminal sends it
//   ESC * + Z : ; Y y    erase the page      ESC T t      erase the line
//   ESC X                erase every page
//   ESC Q W              insert, delete a character
//   ESC E R              insert, delete a line
//   ESC 1 2 3            tab stops; ESC i tab, ESC I back tab
//   ESC j k l m n o p q  the attributes reverse, underline, flash, blank on, off
//   ESC A a              write attribute mode    ESC ) (   write protect mode
//   ESC & '              protect mode            ESC B C   block, conversation
//   ESC c d              program mode            ESC v w   auto flip
//   ESC # "              keyboard lock           ESC N F   next, first page
//   ESC G x              a line drawing character
//   ESC f r c r c        protect bit and attributes of a field
//   ESC 4 5 6 7          send the line, the page; 6 and 7 the protected cells too
// Every other escape sequence does nothing.  That goes for the four of the
// printer port (ESC P, O, J, K), which this terminal has not, and for the
// programmable keys: ESC e and its two parameters are taken and dropped.
//
// A key is one of the codes of term_keyboard.v.  A character that is typed
// goes to the computer; in block mode it goes to the screen instead, as if
// the computer had sent it.  The codes from 84 to 97 hex are the terminal's
// own keys and are carried out at once: each stands for an escape function.
// Those from A0 hex on are the escape function their low seven bits are the
// letter of, and take their turn like a character received.  82 hex is
// CTRL+CLEAR: it unlocks the keyboard, takes a message off the status line
// and is the only key a locked keyboard has.
//
// A cell of the screen memory is twelve bits: the character, the protect bit
// and four attributes.  Characters are written with the protect bit that write
// protect mode gives them, and with the attributes only in write attribute
// mode; a cell that scrolls or is erased to the end of a line keeps the
// attributes it had.
//
// Nothing is moved to scroll.  Each page has the row of memory that is its last
// line; line n of the screen is row (that + 1 + n) mod 24.
//
// The status line is the 80 cells behind the first page's rows.  It is
// written anew behind every character or key that changes what it says; a
// message (MODE ERR, CURS ERR, PROT ERR) flashes until CTRL+CLEAR.
//
// The firmware can go round a loop for ever (ESC X in protect mode with auto
// flip, if the last page has no cell left that is not protected).  The
// controller then stops as the model does: it takes every character and key
// and drops it, until it is reset.
//
// With PAGES = 1 there is one page.  The page functions then stay on it: ESC N
// and ESC F leave the cursor where it is, and with auto flip the cursor goes
// from the last line to the first, and the screen does not scroll.

module term_ampex #(
	parameter PAGES = 2
) (
	input wire clk,
	input wire reset,
	input wire frame,  // one clock a frame of the display

	// characters from the computer
	input  wire [6:0] din,
	input  wire       din_valid,
	output wire       din_ready,

	// keys, by the codes of term_keyboard.v
	input  wire [7:0] key,
	input  wire       key_valid,
	output wire       key_ready,

	// characters for the computer
	output reg  [6:0] dout,
	output reg        dout_valid,
	input  wire       dout_ready,

	// screen memory: the page, then memory row * 80 + column, or 1920 + column
	// for the status line; rdata is the cell at addr, one clock later.  A cell
	// is the four attributes, the protect bit and the character.
	output wire [11:0] addr,
	output wire [11:0] wdata,
	output wire        we_char,
	output wire        we_attr,
	input  wire [11:0] rdata,

	output reg  [4:0] top_row,     // the memory row of the first line
	output wire       page,        // the page shown
	output wire [6:0] cur_x,
	output wire [4:0] cur_y,       // the memory row of the cursor
	output wire       cur_status,  // the cursor is in the status line
	output wire       prog_mode,   // program mode
	output wire       bell         // the bell sounds
);

	localparam TWO = (PAGES == 2);

	localparam [6:0] ESC = 7'h1B;

	// the ways the cursor moves
	localparam [1:0] RIGHT = 2'd0;
	localparam [1:0] LEFT  = 2'd1;
	localparam [1:0] DOWN  = 2'd2;
	localparam [1:0] UP    = 2'd3;

	// ---- the steps.  A routine that others use is entered with `call` and
	// goes back with `leave`.
	localparam [6:0] S_INIT  = 7'd0;
	localparam [6:0] S_IDLE  = 7'd1;
	localparam [6:0] S_RX    = 7'd2;
	localparam [6:0] S_KEY   = 7'd3;
	localparam [6:0] S_COL   = 7'd4;
	localparam [6:0] S_PUMP  = 7'd5;
	localparam [6:0] S_PUT   = 7'd6;
	localparam [6:0] S_PG1   = 7'd7;
	localparam [6:0] S_PG2   = 7'd8;
	localparam [6:0] S_DONE  = 7'd9;
	localparam [6:0] S_NEXT  = 7'd10;
	localparam [6:0] S_END   = 7'd11;
	localparam [6:0] S_CTL   = 7'd12;
	localparam [6:0] S_ESC   = 7'd13;
	localparam [6:0] S_MOVE  = 7'd14;
	localparam [6:0] S_MOVE2 = 7'd15;
	localparam [6:0] S_MOVE3 = 7'd16;
	localparam [6:0] S_MOVE4 = 7'd17;
	localparam [6:0] S_FLIP  = 7'd18;
	localparam [6:0] S_FLIP2 = 7'd19;
	localparam [6:0] S_FILL  = 7'd20;
	localparam [6:0] S_SEEK  = 7'd21;
	localparam [6:0] S_SEEK1 = 7'd22;
	localparam [6:0] S_SEEKF = 7'd23;
	localparam [6:0] S_SEEKW = 7'd24;
	localparam [6:0] S_SEEKT = 7'd25;
	localparam [6:0] S_TAB   = 7'd26;
	localparam [6:0] S_TF1   = 7'd27;
	localparam [6:0] S_BT0   = 7'd28;
	localparam [6:0] S_BT1   = 7'd29;
	localparam [6:0] S_BT2   = 7'd30;
	localparam [6:0] S_BT3   = 7'd31;
	localparam [6:0] S_CP0   = 7'd32;
	localparam [6:0] S_CP1   = 7'd33;
	localparam [6:0] S_CP2   = 7'd34;
	localparam [6:0] S_CP3   = 7'd35;
	localparam [6:0] S_SF0   = 7'd36;
	localparam [6:0] S_SF1   = 7'd37;
	localparam [6:0] S_CA0   = 7'd38;
	localparam [6:0] S_ASK   = 7'd39;
	localparam [6:0] S_EF1   = 7'd40;
	localparam [6:0] S_ERA0  = 7'd41;
	localparam [6:0] S_ERA1  = 7'd42;
	localparam [6:0] S_ERAR  = 7'd43;
	localparam [6:0] S_ERA3  = 7'd44;
	localparam [6:0] S_EOL   = 7'd45;
	localparam [6:0] S_EOLT  = 7'd46;
	localparam [6:0] S_EOT   = 7'd47;
	localparam [6:0] S_CLA0  = 7'd48;
	localparam [6:0] S_CLA1  = 7'd49;
	localparam [6:0] S_CLA2  = 7'd50;
	localparam [6:0] S_CLA3  = 7'd51;
	localparam [6:0] S_CLA4  = 7'd52;
	localparam [6:0] S_ID0   = 7'd53;
	localparam [6:0] S_IDFE  = 7'd54;
	localparam [6:0] S_ID1   = 7'd55;
	localparam [6:0] S_ID2   = 7'd56;
	localparam [6:0] S_ID3   = 7'd57;
	localparam [6:0] S_ID4   = 7'd58;
	localparam [6:0] S_IDB   = 7'd59;
	localparam [6:0] S_IDB2  = 7'd60;
	localparam [6:0] S_IL0   = 7'd61;
	localparam [6:0] S_IL1   = 7'd62;
	localparam [6:0] S_IL2   = 7'd63;
	localparam [6:0] S_IL3   = 7'd64;
	localparam [6:0] S_IL4   = 7'd65;
	localparam [6:0] S_IL5   = 7'd66;
	localparam [6:0] S_IL7   = 7'd67;
	localparam [6:0] S_DL0   = 7'd68;
	localparam [6:0] S_DL1   = 7'd69;
	localparam [6:0] S_DL2   = 7'd70;
	localparam [6:0] S_DL3   = 7'd71;
	localparam [6:0] S_DL4   = 7'd72;
	localparam [6:0] S_DL5   = 7'd73;
	localparam [6:0] S_DL6   = 7'd74;
	localparam [6:0] S_DL7   = 7'd75;
	localparam [6:0] S_DL8   = 7'd76;
	localparam [6:0] S_DL9   = 7'd77;
	localparam [6:0] S_CPR0  = 7'd78;
	localparam [6:0] S_CPR1  = 7'd79;
	localparam [6:0] S_CPRR  = 7'd80;
	localparam [6:0] S_CPRW  = 7'd81;
	localparam [6:0] S_SND0  = 7'd82;
	localparam [6:0] S_SND1  = 7'd83;
	localparam [6:0] S_SNDA  = 7'd84;
	localparam [6:0] S_SNDA1 = 7'd85;
	localparam [6:0] S_SNDA2 = 7'd86;
	localparam [6:0] S_SNDA3 = 7'd87;
	localparam [6:0] S_SNDP  = 7'd88;
	localparam [6:0] S_SNDP1 = 7'd89;
	localparam [6:0] S_SNDQ  = 7'd90;
	localparam [6:0] S_SNDQ1 = 7'd91;
	localparam [6:0] S_SNDQ2 = 7'd92;
	localparam [6:0] S_SNDC  = 7'd93;
	localparam [6:0] S_SNDD  = 7'd94;
	localparam [6:0] S_SNDR  = 7'd95;
	localparam [6:0] S_SNDE  = 7'd96;
	localparam [6:0] S_ST0   = 7'd97;
	localparam [6:0] S_ST1   = 7'd98;
	localparam [6:0] S_ST2   = 7'd99;
	localparam [6:0] S_TX    = 7'd100;
	localparam [6:0] S_W2    = 7'd101;
	localparam [6:0] S_W1    = 7'd102;

	reg [6:0] state;
	reg [6:0] ret;  // where the two steps that wait for the memory go on
	reg [6:0] tx_ret;  // where it goes on when the computer has taken a character
	reg [6:0] stk0, stk1, stk2;  // where the routines go back to

	// ---- what the terminal remembers
	reg [4:0] cur_r;  // the cursor: its row of memory
	reg [6:0] cur_c;  // and its column
	reg       in_status;  // the cursor is shown in the status line
	reg       pg;  // the page the cursor is on
	reg       vis;  // the page shown
	reg       cpu;  // the page that is read and written
	reg [4:0] bot0, bot1;  // for each page, the row of memory that is its last line
	reg m_flp, m_atb, m_wpt, m_blk, m_prt, m_pgm;  // the modes
	reg [ 3:0] at;  // the attributes switched on: underline, flash, blank, reverse
	reg        locked;  // the keyboard is locked
	// A tab stop for each column.  Those of columns 72 to 79 are also what the
	// firmware keeps of an erase, whether it leaves protected cells alone
	// (bit 72): every erase sets or clears all eight.
	reg [79:0] tabs;
	reg        advance;  // the last character moves the cursor right
	reg        dirty;  // the status line is to be written anew
	reg [ 2:0] err;  // the message it gets: errors are ORed together
	reg        hung;  // the firmware would go round for ever

	// The collector of escape sequences: the character behind ESC says how
	// many more belong to the sequence.  Nothing looks at what a parameter is.
	reg       esc_seen;
	reg [2:0] esc_left;
	reg [2:0] esc_at;
	// What is worked off: a character, or a sequence with its parameters.  A
	// key of the terminal's own and the character behind ESC have bit 7.
	reg [7:0] q0;
	reg [6:0] c0;  // the letter of a sequence that is not complete yet
	reg [6:0] q1, q2, q3, q4;
	reg [2:0] qn;
	reg [2:0] qi;

	reg [6:0] ch;  // the character received
	reg [7:0] kc;  // the key
	reg       key_wait;  // a key came with a character and is worked off behind it

	// A place on the screen that most routines work with, and the limit and the
	// direction of its walk.  Limit and direction are left as the last routine
	// had them, and two routines use them without setting them.
	reg [4:0] pr;
	reg [6:0] pc;
	reg       poff;  // it is beside the eighty columns: nothing is there
	reg [4:0] lr;
	reg [6:0] lc;
	reg       back;
	reg [4:0] er;  // where a send ends
	reg [6:0] ec;

	reg       sk_want;  // a search: for a protected cell, and its direction
	reg       sk_back;
	reg       want;
	reg       wrapped;  // the search went round the end of the page
	reg       seek2;  // its walk has reached its limit
	reg [1:0] pdone;  // the pages it has gone through, less one

	reg       down;  // the cursor moves down a line, or up
	reg [1:0] mv;
	reg       was_bottom;  // it was on the last line
	reg       flip_to;
	reg [4:0] tl;

	reg       raw;  // cells are written whole: no protect bit, no attributes
	reg       fill_sp;  // an erase fills with blanks, not with empty cells
	reg       er_whole;
	reg       er_all;
	reg [4:0] el;  // the line an erase is on
	reg       sv_pg;
	reg [4:0] rounds;
	reg       cp_set;
	reg       id_ins;  // a character is inserted, not deleted
	reg [6:0] id_end;
	reg [6:0] id_cnt;
	reg [4:0] lrow;  // the cursor's line
	reg [4:0] ln;
	reg cp_sp, cp_dp;  // a line is copied: from this page to that
	reg [4:0] cp_sl, cp_dl;  // from this line to that
	reg [4:0] sm, dm;
	reg [11:0] hold;  // a cell on its way
	reg [ 6:0] val;  // a character on its way to the cell under the cursor

	reg       sending;  // a send is asked for
	reg       snd_home;  // from the first line on, not from the start of the cursor's
	reg       send_all;  // with the protected cells
	reg [3:0] s_at;  // the attributes and the protect bit of the cell sent last
	reg       s_pr;
	reg [3:0] achg, awas;
	reg [1:0] ai;

	reg [6:0] st_col;  // the status line is written column by column
	reg [5:0] st_mask;
	reg [3:0] st_am;
	reg       st_fdx;
	reg       st_init;
	reg       a_st;  // the memory is addressed at the status line

	reg ring;  // the bell is rung

	// ---- lines and rows
	// the line of the screen a row of memory is
	function [4:0] line_of;
		input [4:0] row;
		input [4:0] bottom;
		line_of = (bottom < row) ? row - bottom - 5'd1 : row + (5'd23 - bottom);
	endfunction
	// the row of memory a line of the screen is
	function [4:0] row_of;
		input [4:0] line;
		input [4:0] bottom;
		reg [5:0] sum;
		begin
			sum    = {1'b0, line} + {1'b0, bottom} + 6'd1;
			row_of = (sum >= 6'd24) ? sum[4:0] - 5'd24 : sum[4:0];
		end
	endfunction

	wire [4:0] bot = pg ? bot1 : bot0;
	wire [4:0] home_r = (bot == 5'd23) ? 5'd0 : bot + 5'd1;
	wire [4:0] cur_line = line_of(cur_r, bot);
	wire [4:0] last_line = line_of(5'd23, bot);
	wire       pg_next = TWO ? ~pg : 1'b0;
	wire       pg_last = (m_flp && TWO) ? 1'b1 : pg;  // the last page a line insert or delete goes through
	wire       chain = m_flp && TWO && !pg;

	// ---- a step of the pointer.  Rows go round the memory, 23 to 0, not round
	// the screen.
	wire       f_wrap = (pc == 7'd79);
	wire       b_wrap = (pc == 7'd0);
	wire [4:0] f_r = !f_wrap ? pr : (pr == 5'd23) ? 5'd0 : pr + 5'd1;
	wire [6:0] f_c = f_wrap ? 7'd0 : pc + 7'd1;
	wire [4:0] b_r = !b_wrap ? pr : (pr == 5'd0) ? 5'd23 : pr - 5'd1;
	wire [6:0] b_c = b_wrap ? 7'd79 : pc - 7'd1;
	wire [4:0] n_r = back ? b_r : f_r;
	wire [6:0] n_c = back ? b_c : f_c;
	wire       n_wrap = back ? b_wrap : f_wrap;
	wire       at_limit = (pr == lr) && (pc == lc);
	wire       at_end = (pr == er) && (pc == ec);

	// ---- the memory.  The address follows the pointer, a clock behind: a
	// write that a step asks for is of the cell the pointer is at in that
	// step, and rdata is that cell two steps later.
	reg [11:0] addr_r;
	reg [11:0] wdata_r;
	reg we_c, we_a;
	wire [10:0] lin = {pr, 6'd0} + {2'd0, pr, 4'd0} + {4'd0, pc};
	always @(posedge clk) addr_r <= a_st ? {1'b0, 11'd1920 + {4'd0, st_col}} : {cpu, lin};
	assign addr    = addr_r;
	assign wdata   = wdata_r;
	assign we_char = we_c;
	assign we_attr = we_a;

	// ---- what is on its way
	reg [6:0] qp;
	always @(*)
		case (qi)
			3'd1:    qp = q1;
			3'd2:    qp = q2;
			3'd3:    qp = q3;
			default: qp = q4;
		endcase
	wire [7:0] cb = (qi == 3'd0) ? q0 : {1'b0, qp};

	// The cell that shows a control character in program mode: twenty-one have
	// a picture of their own, the others are shown as line drawing characters.
	function [6:0] picture;
		input [6:0] c;
		case (c)
			7'h01:   picture = 7'h0E;
			7'h02:   picture = 7'h0F;
			7'h03:   picture = 7'h10;
			7'h06:   picture = 7'h12;
			7'h07:   picture = 7'h14;
			7'h08:   picture = 7'h16;
			7'h09:   picture = 7'h17;
			7'h0A:   picture = 7'h19;
			7'h0B:   picture = 7'h1A;
			7'h0E:   picture = 7'h03;
			7'h0F:   picture = 7'h01;
			7'h10:   picture = 7'h02;
			7'h12:   picture = 7'h06;
			7'h14:   picture = 7'h07;
			7'h16:   picture = 7'h08;
			7'h17:   picture = 7'h09;
			7'h19:   picture = 7'h0A;
			7'h1A:   picture = 7'h0B;
			default: picture = c;
		endcase
	endfunction
	// and back, for what is sent of the screen
	function [6:0] pictured;
		input [6:0] c;
		case (c)
			7'h0E:   pictured = 7'h01;
			7'h0F:   pictured = 7'h02;
			7'h10:   pictured = 7'h03;
			7'h12:   pictured = 7'h06;
			7'h14:   pictured = 7'h07;
			7'h16:   pictured = 7'h08;
			7'h17:   pictured = 7'h09;
			7'h19:   pictured = 7'h0A;
			7'h1A:   pictured = 7'h0B;
			7'h03:   pictured = 7'h0E;
			7'h01:   pictured = 7'h0F;
			7'h02:   pictured = 7'h10;
			7'h06:   pictured = 7'h12;
			7'h07:   pictured = 7'h14;
			7'h08:   pictured = 7'h16;
			7'h09:   pictured = 7'h17;
			7'h0A:   pictured = 7'h19;
			7'h0B:   pictured = 7'h1A;
			default: pictured = c;
		endcase
	endfunction

	// the escape functions that are carried out in program mode too
	wire [7:0] letter = {1'b0, q0[6:0]};
	wire [7:0] ch_w = {1'b0, ch};
	wire works = (letter >= "4" && letter <= "7") || letter == "P" || letter == "T" || letter == "Y" || letter == "d" || letter == "t" || letter == "y";

	// the escape function one of the terminal's own keys stands for
	reg [7:0] key_fn;
	always @(*)
		case (kc)
			8'h84:   key_fn = "B";
			8'h85:   key_fn = "C";
			8'h86:   key_fn = ")";
			8'h87:   key_fn = "(";
			8'h88:   key_fn = "&";
			8'h89:   key_fn = "'";
			8'h8A:   key_fn = "N";
			8'h8B:   key_fn = ";";
			8'h8C:   key_fn = ":";
			8'h90:   key_fn = "I";
			8'h91:   key_fn = "1";
			8'h92:   key_fn = "2";
			8'h93:   key_fn = "3";
			8'h94:   key_fn = "E";
			8'h95:   key_fn = "R";
			8'h96:   key_fn = "Q";
			8'h97:   key_fn = "W";
			default: key_fn = 8'd0;
		endcase

	// ---- ESC = row column.  The row is checked only after the page's last
	// row has been added and 24 taken off once: on a screen that has not
	// scrolled rows 0 to 23 are right, and row bytes below 20 hex are taken as
	// well; on one that has scrolled, some rows above 23 are.
	wire [7:0] ca_sum = {1'b0, q1} + {3'd0, bot} + 8'd1;
	wire [7:0] ca_off = ca_sum - 8'd8;
	// (less 48, 24 or nothing, in five bits)
	wire [4:0] ca_row = (ca_off >= 8'd48) ? ca_off[4:0] - 5'd16 : (ca_off >= 8'd24) ? ca_off[4:0] - 5'd24 : ca_off[4:0];
	wire [6:0] ca_col = q2 - 7'h20;
	wire ca_ok = (ca_sum >= 8'd8) && (ca_sum <= 8'd79) && (q2 >= 7'h20) && (q2 <= 7'h6F);

	// ---- ESC f: from a line and column to a line and column
	wire [6:0] f_r1 = q1 - 7'h20;
	wire [6:0] f_c1 = q2 - 7'h20;
	wire [6:0] f_r2 = q3 - 7'h20;
	wire [6:0] f_c2 = q4 - 7'h20;
	wire f_ok = (m_atb || m_wpt) && (q1 >= 7'h20) && (q1 <= 7'h37) && (q3 >= 7'h20) && (q3 <= 7'h37) && (q2 >= 7'h20) &&
		(q2 <= 7'h6F) && (q4 >= 7'h20) && (q4 <= 7'h6F) && (f_r2 >= f_r1) && !((f_r1 == f_r2) && (f_c2 < f_c1));

	// ---- the status line.  Columns 13 to 61 and 76 to 79 say how the
	// terminal is set; 67 to 74 hold a message, which flashes.
	//
	// The names of the modes that are on stand one behind the other from
	// column 30 on, four columns each, and FDX behind them: the terminal is
	// never set to half duplex.
	wire [ 2:0] st_id = st_mask[0] ? 3'd0 : st_mask[1] ? 3'd1 : st_mask[2] ? 3'd2 : st_mask[3] ? 3'd3 : st_mask[4] ? 3'd4 :
		st_mask[5] ? 3'd5 : st_fdx ? 3'd7 : 3'd6;
	reg [23:0] st_name;
	always @(*)
		case (st_id)
			3'd0:    st_name = "FLP";
			3'd1:    st_name = "ATB";
			3'd2:    st_name = "WPT";
			3'd3:    st_name = "BLK";
			3'd4:    st_name = "PRT";
			3'd5:    st_name = "PGM";
			3'd6:    st_name = "FDX";
			default: st_name = "   ";
		endcase
	wire [1:0] st_ph = st_col[1:0] ^ 2'b10;  // the column within a name
	// Errors are ORed together as numbers, as the firmware has it: the cursor
	// address off the screen, 4, and no unprotected cell, 5, are 5.
	wire [63:0] st_msg = (err == 3'd2) ? "MODE ERR" : (err == 3'd4) ? "CURS ERR" : (err == 3'd5) ? "PROT ERR" : "        ";
	reg [7:0] st_ch;  // the protect bit, which is off, and the character
	reg st_we;
	reg st_fl;
	always @(*) begin
		st_ch = " ";
		st_we = dirty;
		st_fl = 1'b0;
		case (st_col)
			// CTRL+CLEAR takes a message away, and blanks these
			7'd0, 7'd1, 7'd2, 7'd3, 7'd4, 7'd5, 7'd6, 7'd7, 7'd8, 7'd9, 7'd10, 7'd11, 7'd12: begin
				st_we = (err == 3'd1);
				st_fl = 1'b1;
			end
			7'd13: st_ch = "A";
			7'd14: st_ch = "T";
			7'd15: st_ch = "T";
			7'd16: st_ch = "R";
			7'd17: st_ch = "B";
			7'd18: st_ch = "S";
			7'd19: st_ch = ":";
			// a letter for each attribute that is on, one behind the other
			7'd20, 7'd21, 7'd22, 7'd23: st_ch = st_am[0] ? "R" : st_am[1] ? "B" : st_am[2] ? "F" : st_am[3] ? "U" : " ";
			7'd25: st_ch = "M";
			7'd26: st_ch = "O";
			7'd27: st_ch = "D";
			7'd28: st_ch = "E";
			7'd29: st_ch = ":";
			7'd58: st_ch = locked ? "L" : " ";
			7'd59: st_ch = locked ? "O" : " ";
			7'd60: st_ch = locked ? "C" : " ";
			7'd61: st_ch = locked ? "K" : " ";
			// these three are never written
			7'd62, 7'd66, 7'd75: st_we = 1'b0;
			7'd67, 7'd68, 7'd69, 7'd70, 7'd71, 7'd72, 7'd73, 7'd74: begin
				st_we = (err != 3'd0);
				st_fl = 1'b1;
				case (st_col[2:0])
					3'd3:    st_ch = st_msg[63:56];
					3'd4:    st_ch = st_msg[55:48];
					3'd5:    st_ch = st_msg[47:40];
					3'd6:    st_ch = st_msg[39:32];
					3'd7:    st_ch = st_msg[31:24];
					3'd0:    st_ch = st_msg[23:16];
					3'd1:    st_ch = st_msg[15:8];
					default: st_ch = st_msg[7:0];
				endcase
			end
			7'd76: st_ch = "P";
			7'd77: st_ch = "G";
			7'd79: st_ch = pg ? "2" : "1";
			default:
			if (st_col >= 7'd30 && st_col <= 7'd57)
				case (st_ph)
					2'd0:    st_ch = st_name[23:16];
					2'd1:    st_ch = st_name[15:8];
					2'd2:    st_ch = st_name[7:0];
					default: st_ch = " ";
				endcase
		endcase
		// after a reset every column is blanked first
		if (st_init) begin
			st_ch = " ";
			st_we = 1'b1;
			st_fl = 1'b0;
		end
	end

	// the letter that switches an attribute on or off: j k for reverse, p q for
	// blank, n o for flash, l m for underline
	reg [5:0] at_letter;
	always @(*)
		case (ai)
			2'd0:    at_letter = 6'b110101;
			2'd1:    at_letter = 6'b111000;
			2'd2:    at_letter = 6'b110111;
			default: at_letter = 6'b110110;
		endcase

	// ---- the routines
	task call;
		input [6:0] to;
		input [6:0] from;
		begin
			state <= to;
			stk0  <= from;
			stk1  <= stk0;
			stk2  <= stk1;
		end
	endtask
	task leave;
		begin
			state <= stk0;
			stk0  <= stk1;
			stk1  <= stk2;
		end
	endtask
	// go on when the cell the pointer is at now has been read
	task after1;
		input [6:0] to;
		begin
			state <= S_W1;
			ret   <= to;
		end
	endtask
	// go on when the cell the pointer is set to in this step has been read
	task after2;
		input [6:0] to;
		begin
			state <= S_W2;
			ret   <= to;
		end
	endtask
	// a character into the cell the pointer is at, as the terminal writes
	// characters
	task put;
		input [6:0] c;
		begin
			wdata_r <= raw ? {5'd0, c} : {at, m_wpt, c};
			we_c    <= ~poff;
			we_a    <= (raw | m_atb) & ~poff;
		end
	endtask
	// a whole cell
	task put_all;
		input [11:0] c;
		begin
			wdata_r <= c;
			we_c    <= 1'b1;
			we_a    <= 1'b1;
		end
	endtask
	task emit;
		input [6:0] c;
		input [6:0] to;
		begin
			dout       <= c;
			dout_valid <= 1'b1;
			state      <= S_TX;
			tx_ret     <= to;
		end
	endtask
	// In protect mode: from the cursor on, forward or backward, to the first
	// cell that is protected (w) or is not.
	task seek;
		input w;
		input b;
		input [6:0] from;
		begin
			sk_want <= w;
			sk_back <= b;
			call(S_SEEK, from);
		end
	endtask
	task erase;
		input sp;
		input keep;
		input whole;
		begin
			fill_sp     <= sp;
			tabs[79:72] <= {8{keep}};
			er_whole    <= whole;
			call(S_ERA0, S_DONE);
		end
	endtask
	task tab;
		begin
			if (m_prt) seek(1'b1, 1'b0, S_TF1);
			else begin
				pc    <= cur_c + 7'd1;
				state <= S_TAB;
			end
		end
	endtask
	task copy;
		input sp;
		input [4:0] sl;
		input dp;
		input [4:0] dl;
		input [6:0] from;
		begin
			cp_sp <= sp;
			cp_sl <= sl;
			cp_dp <= dp;
			cp_dl <= dl;
			call(S_CPR0, from);
		end
	endtask

	assign din_ready = (state == S_IDLE) && !key_wait;
	assign key_ready = din_ready;

	always @(posedge clk) begin
		we_c <= 1'b0;
		we_a <= 1'b0;
		ring <= 1'b0;
		if (reset) begin
			state      <= S_INIT;
			cur_r      <= 5'd0;
			cur_c      <= 7'd0;
			in_status  <= 1'b0;
			pg         <= 1'b0;
			vis        <= 1'b0;
			cpu        <= 1'b0;
			bot0       <= 5'd23;
			bot1       <= 5'd23;
			m_flp      <= 1'b0;
			m_atb      <= 1'b0;
			m_wpt      <= 1'b0;
			m_blk      <= 1'b0;
			m_prt      <= 1'b0;
			m_pgm      <= 1'b0;
			at         <= 4'd0;
			locked     <= 1'b0;
			tabs       <= 80'd0;
			advance    <= 1'b0;
			dirty      <= 1'b1;
			err        <= 3'd0;
			hung       <= 1'b0;
			esc_seen   <= 1'b0;
			esc_left   <= 3'd0;
			key_wait   <= 1'b0;
			pr         <= 5'd0;
			pc         <= 7'd0;
			poff       <= 1'b0;
			seek2      <= 1'b0;
			raw        <= 1'b1;
			sending    <= 1'b0;
			st_init    <= 1'b0;
			a_st       <= 1'b0;
			dout_valid <= 1'b0;
		end else
			case (state)
				// after a reset: every cell of every page empty
				S_INIT: begin
					put(7'd0);
					pr <= f_r;
					pc <= f_c;
					if (pr == 5'd23 && pc == 7'd79) begin
						if (TWO && !cpu) cpu <= 1'b1;
						else begin
							raw     <= 1'b0;
							cpu     <= 1'b0;
							st_init <= 1'b1;
							state   <= S_ST0;
						end
					end
				end

				S_IDLE: begin
					if (key_wait) begin
						key_wait <= 1'b0;
						if (!hung) state <= S_KEY;
					end else if (din_valid) begin
						ch <= din;
						if (key_valid && !hung) begin
							kc       <= key;
							key_wait <= 1'b1;
						end
						if (!hung) state <= S_RX;
					end else if (key_valid) begin
						kc <= key;
						if (!hung) state <= S_KEY;
					end
				end

				// A character from the computer.  NUL is dropped, but in
				// program mode.
				S_RX: begin
					qi <= 3'd0;
					qn <= 3'd1;
					if (ch == 7'd0 && !m_pgm) state <= S_IDLE;
					else if (ch == ESC || esc_seen || esc_left != 3'd0) state <= S_COL;
					else begin
						q0    <= {1'b0, ch};
						state <= S_PUMP;
					end
				end

				// A key.  The terminal's own keys act on the screen; a character
				// goes to the computer, and in block mode to the screen instead,
				// through the collector like one received.
				S_KEY: begin
					qi <= 3'd0;
					qn <= 3'd1;
					if (kc == 8'h82) begin
						// CTRL+CLEAR unlocks the keyboard and takes a message away
						locked    <= 1'b0;
						dirty     <= 1'b1;
						err       <= 3'd1;
						in_status <= 1'b0;
						seek(1'b0, 1'b0, S_END);
					end else if (locked) state <= S_IDLE;
					else if (kc >= 8'hA0) begin
						q0    <= kc;
						state <= S_PUMP;
					end else if (kc[7]) begin
						if ((kc >= 8'h90 && m_pgm) || key_fn == 8'd0) state <= S_IDLE;
						else begin
							q0    <= {1'b1, key_fn[6:0]};
							state <= S_ESC;
						end
					end else if (m_blk) begin
						ch <= kc[6:0];
						if (kc[6:0] == ESC || esc_seen || esc_left != 3'd0) state <= S_COL;
						else begin
							q0    <= kc;
							state <= S_PUMP;
						end
					end else emit(kc[6:0], S_IDLE);
				end

				S_COL: begin
					if (esc_seen) begin
						esc_seen <= 1'b0;
						esc_at   <= 3'd1;
						c0       <= ch;
						q0       <= {1'b1, ch};
						state    <= S_IDLE;
						if (ch_w == "G") esc_left <= 3'd1;
						else if (ch_w == "=" || ch_w == "e") esc_left <= 3'd2;
						else if (ch_w == "f") esc_left <= 3'd4;
						else state <= S_PUMP;
					end else if (esc_left != 3'd0) begin
						case (esc_at)
							3'd1:    q1 <= ch;
							3'd2:    q2 <= ch;
							3'd3:    q3 <= ch;
							default: q4 <= ch;
						endcase
						esc_at   <= esc_at + 3'd1;
						esc_left <= esc_left - 3'd1;
						qn       <= esc_at + 3'd1;
						q0       <= {1'b1, c0};
						state    <= (esc_left == 3'd1) ? S_PUMP : S_IDLE;
					end else begin
						esc_seen <= 1'b1;
						state    <= S_IDLE;
					end
				end

				// What was received, one after the other.  In program mode
				// everything is shown, but for a few escape functions.
				S_PUMP: begin
					pr <= cur_r;
					pc <= cur_c;
					if (m_pgm) begin
						if (cb[7] && cb[6:5] != 2'b00 && works) state <= S_ESC;
						else if (cb[7]) begin
							// the picture of ESC, then the letter as a character
							val <= ESC;
							call(S_PUT, S_PG1);
						end else begin
							val     <= picture(cb[6:0]);
							advance <= 1'b1;
							call(S_PUT, S_DONE);
						end
					end else if (cb[7]) state <= S_ESC;
					else if (cb[6:5] == 2'b00) state <= S_CTL;
					else if (cb[6:0] == 7'h7F) state <= S_NEXT;
					else begin
						val     <= cb[6:0];
						advance <= 1'b1;
						call(S_PUT, S_DONE);
					end
				end

				S_PUT: begin
					put(val);
					leave;
				end

				S_PG1: begin
					mv <= RIGHT;
					call(S_MOVE, S_PG2);
				end

				S_PG2: begin
					pr      <= cur_r;
					pc      <= cur_c;
					val     <= picture(q0[6:0]);
					advance <= 1'b1;
					call(S_PUT, S_DONE);
				end

				// the cursor one to the right behind a character
				S_DONE: begin
					mv <= RIGHT;
					if (advance) call(S_MOVE, S_NEXT);
					else state <= S_NEXT;
				end

				S_NEXT: begin
					qi    <= qi + 3'd1;
					state <= (qi + 3'd1 < qn) ? S_PUMP : S_END;
				end

				// the status line, then what is to be sent
				S_END: state <= (dirty || err != 3'd0) ? S_ST0 : sending ? S_SND0 : S_IDLE;

				// ---- a control character
				S_CTL: begin
					advance <= 1'b0;
					state   <= S_DONE;
					case (cb[4:0])
						5'h07:   ring <= 1'b1;
						5'h08: begin
							mv <= LEFT;
							call(S_MOVE, S_DONE);
						end
						5'h09:   tab;
						5'h0A: begin
							mv <= DOWN;
							call(S_MOVE, S_DONE);
						end
						5'h0B: begin
							mv <= UP;
							call(S_MOVE, S_DONE);
						end
						// cursor right, as behind a character
						5'h0C:   advance <= 1'b1;
						5'h0D: begin
							cur_c <= 7'd0;
							seek(1'b0, 1'b0, S_DONE);
						end
						// the whole page to blanks, protected cells too
						5'h1A:   erase(1'b1, 1'b0, 1'b1);
						5'h1E: begin
							cur_c <= 7'd0;
							cur_r <= home_r;
							seek(1'b0, 1'b0, S_DONE);
						end
						5'h1F: begin
							cur_c <= 7'd0;
							mv    <= DOWN;
							call(S_MOVE, S_DONE);
						end
						default: ;
					endcase
				end

				// ---- an escape function, by its letter.  ESC and a control
				// character is nothing at all, not even the end of the cursor's
				// move behind the character before: the cursor moves once more.
				S_ESC: begin
					state <= S_DONE;
					qn    <= 3'd0;
					if (q0[6:5] != 2'b00) begin
						advance <= 1'b0;
						case (letter)
							"\"": begin
								locked <= 1'b0;
								dirty  <= 1'b1;
							end
							"#": begin
								locked <= 1'b1;
								dirty  <= 1'b1;
							end
							"&":
							if (m_pgm) err <= err | 3'd2;
							else begin
								m_prt <= 1'b1;
								dirty <= 1'b1;
								seek(1'b0, 1'b0, S_DONE);
							end
							"'": begin
								m_prt <= 1'b0;
								dirty <= 1'b1;
							end
							"(": begin
								m_wpt <= 1'b0;
								dirty <= 1'b1;
							end
							")":
							if (m_pgm) err <= err | 3'd2;
							else begin
								m_wpt <= 1'b1;
								dirty <= 1'b1;
							end
							"*":      erase(1'b0, 1'b0, 1'b1);
							"+", "Z": erase(1'b1, 1'b0, 1'b1);
							":":      erase(1'b0, 1'b1, 1'b1);
							";":      erase(1'b1, 1'b1, 1'b1);
							"Y":      erase(1'b1, 1'b1, 1'b0);
							"y":      erase(1'b0, 1'b1, 1'b0);
							"1", "2":
							if (m_prt) begin
								cp_set <= q0[0];
								state  <= S_CP0;
							end else tabs[cur_c] <= q0[0];
							"3":      if (!m_prt) tabs <= 80'd0;
							"4", "5", "6", "7": begin
								sending  <= 1'b1;
								snd_home <= q0[0];
								send_all <= q0[1];
							end
							"=":      state <= S_CA0;
							"?":      emit({2'b01, cur_line}, S_ASK);
							"A": begin
								m_atb <= 1'b1;
								dirty <= 1'b1;
							end
							"a": begin
								m_atb <= 1'b0;
								dirty <= 1'b1;
							end
							"B": begin
								m_blk <= 1'b1;
								dirty <= 1'b1;
							end
							"C": begin
								m_blk <= 1'b0;
								dirty <= 1'b1;
							end
							"E":      state <= S_IL0;
							"R":      state <= S_DL0;
							"F": begin
								flip_to <= 1'b0;
								call(S_FLIP, S_EF1);
							end
							"N": begin
								flip_to <= pg_next;
								call(S_FLIP, S_EF1);
							end
							"G":
							if (q1 >= 7'h41 && q1 <= 7'h4B) begin
								pr      <= cur_r;
								pc      <= cur_c;
								val     <= q1 - 7'h40;
								advance <= 1'b1;
								call(S_PUT, S_DONE);
							end
							// back tab: in protect mode only
							"I":      if (m_prt) state <= S_BT0;
							"i":      tab;
							"Q": begin
								id_ins <= 1'b1;
								state  <= S_ID0;
							end
							"W": begin
								id_ins <= 1'b0;
								state  <= S_ID0;
							end
							"T", "t": begin
								pr          <= cur_r;
								pc          <= cur_c;
								fill_sp     <= ~q0[5];
								tabs[79:72] <= {8{m_prt}};
								call(S_EOL, S_EOT);
							end
							"X":      state <= S_CLA0;
							"c":
							if (m_prt || m_wpt || m_atb) err <= err | 3'd2;
							else begin
								m_pgm <= 1'b1;
								dirty <= 1'b1;
							end
							"d": begin
								m_pgm <= 1'b0;
								dirty <= 1'b1;
							end
							"f":      state <= S_SF0;
							"j", "k": begin
								at[0] <= ~q0[0];
								dirty <= 1'b1;
							end
							"l", "m": begin
								at[3] <= ~q0[0];
								dirty <= 1'b1;
							end
							"n", "o": begin
								at[2] <= ~q0[0];
								dirty <= 1'b1;
							end
							"p", "q": begin
								at[1] <= ~q0[0];
								dirty <= 1'b1;
							end
							"v": begin
								m_flp <= 1'b1;
								dirty <= 1'b1;
							end
							"w": begin
								m_flp <= 1'b0;
								dirty <= 1'b1;
							end
							default:  ;
						endcase
					end
				end

				S_ASK: emit(cur_c + 7'h20, S_DONE);

				S_EF1: seek(1'b0, 1'b0, S_DONE);

				S_CA0: begin
					in_status <= ~ca_ok;
					if (ca_ok) begin
						cur_r <= ca_row;
						cur_c <= ca_col;
					end else begin
						cur_r <= 5'd0;
						cur_c <= 7'd0;
						err   <= err | 3'd4;
					end
					seek(1'b0, 1'b0, S_DONE);
				end

				// ---- the cursor
				// by a column or a line: right, left, down, up.  Left of the
				// first column is the end of the line above.
				S_MOVE: begin
					case (mv)
						RIGHT: begin
							down <= 1'b1;
							if (cur_c == 7'd79) begin
								cur_c <= 7'd0;
								state <= S_MOVE2;
							end else begin
								cur_c   <= cur_c + 7'd1;
								sk_want <= 1'b0;
								sk_back <= 1'b0;
								state   <= S_SEEK;
							end
						end
						LEFT: begin
							down <= 1'b0;
							if (cur_c == 7'd0) begin
								cur_c <= 7'd79;
								state <= S_MOVE2;
							end else begin
								cur_c   <= cur_c - 7'd1;
								sk_want <= 1'b0;
								sk_back <= 1'b1;
								state   <= S_SEEK;
							end
						end
						DOWN: begin
							down  <= 1'b1;
							state <= S_MOVE2;
						end
						default: begin
							down  <= 1'b0;
							state <= S_MOVE2;
						end
					endcase
				end

				// To the line below or above, round the rows of the memory.  With
				// auto flip the page changes at its end.
				S_MOVE2: begin
					was_bottom <= (cur_r == bot);
					flip_to    <= pg_next;
					if (down) begin
						cur_r <= (cur_r == 5'd23) ? 5'd0 : cur_r + 5'd1;
						if (m_flp && cur_r == bot) call(S_FLIP, S_MOVE3);
						else state <= S_MOVE3;
					end else begin
						cur_r <= (cur_r == 5'd0) ? 5'd23 : cur_r - 5'd1;
						if (m_flp && cur_r == home_r) call(S_FLIP, S_MOVE3);
						else state <= S_MOVE3;
					end
				end

				S_MOVE3: seek(1'b0, ~down, S_MOVE4);

				// The cursor has gone down from the last line and is on the row
				// that was the first.  In conversation mode that row is filled
				// with blanks and becomes the last line: the screen has scrolled.
				// In block mode, in protect mode and with auto flip nothing more
				// happens, so the cursor is on the first line.
				S_MOVE4: begin
					if (was_bottom && down && !m_flp && !m_blk && !m_prt) begin
						pr <= cur_r;
						pc <= 7'd0;
						if (pg) bot1 <= cur_r;
						else bot0 <= cur_r;
						state <= S_FILL;
					end else leave;
				end

				// eighty blanks, written like characters
				S_FILL: begin
					put(7'h20);
					pc <= pc + 7'd1;
					if (pc == 7'd79) leave;
				end

				// to another page: the cursor keeps its line and column
				S_FLIP: begin
					tl    <= cur_line;
					pg    <= flip_to;
					cpu   <= flip_to;
					vis   <= flip_to;
					dirty <= 1'b1;
					state <= S_FLIP2;
				end

				S_FLIP2: begin
					cur_r <= row_of(tl, bot);
					leave;
				end

				// ---- the search of protect mode.  At the end of the page the
				// walk goes on from the other end (of the next page, with auto
				// flip) and from then on looks for a cell that is not protected.
				// If the pages have all been gone through: PROT ERR.
				S_SEEK: begin
					if (!m_prt) leave;
					else begin
						back    <= sk_back;
						want    <= sk_want;
						wrapped <= 1'b0;
						pdone   <= 2'd3;
						pr      <= cur_r;
						pc      <= cur_c;
						lr      <= sk_back ? home_r : bot;
						lc      <= sk_back ? 7'd0 : 7'd79;
						state   <= S_SEEK1;
					end
				end

				S_SEEK1: begin
					if (seek2) begin
						seek2   <= 1'b0;
						wrapped <= 1'b1;
						want    <= 1'b0;
						pdone   <= pdone + 2'd1;
						flip_to <= pg_next;
						if (pdone == (TWO ? 2'd1 : 2'd0)) begin
							err       <= err | 3'd5;
							in_status <= 1'b1;
							cur_c     <= 7'd0;
							leave;
						end else if (m_flp) call(S_FLIP, S_SEEKF);
						else state <= S_SEEKW;
					end else after1(S_SEEKT);
				end

				S_SEEKF: begin
					lr    <= cur_r;
					lc    <= cur_c;
					state <= S_SEEKW;
				end

				S_SEEKW: begin
					pr <= back ? bot : home_r;
					pc <= back ? 7'd79 : 7'd0;
					after2(S_SEEKT);
				end

				S_SEEKT: begin
					if (rdata[7] == want) begin
						cur_r <= pr;
						cur_c <= pc;
						leave;
					end else begin
						if (at_limit) seek2 <= 1'b1;
						else begin
							pr <= n_r;
							pc <= n_c;
						end
						state <= S_SEEK1;
					end
				end

				// ---- tab stops
				// to the next stop on the line; if there is none the cursor stays
				S_TAB: begin
					if (pc >= 7'd80) state <= S_DONE;
					else if (tabs[pc]) begin
						cur_c <= pc;
						state <= S_DONE;
					end else pc <= pc + 7'd1;
				end

				// in protect mode: to the start of the next field
				S_TF1: begin
					if (wrapped) state <= S_DONE;
					else seek(1'b0, 1'b0, S_DONE);
				end

				// back tab: to the start of the field before
				S_BT0: begin
					back  <= 1'b1;
					pr    <= cur_r;
					pc    <= cur_c;
					lr    <= home_r;
					lc    <= 7'd0;
					state <= S_BT1;
				end

				S_BT1: begin
					if (at_limit) seek2 <= 1'b1;
					else begin
						cur_r <= b_r;
						cur_c <= b_c;
					end
					seek(1'b0, 1'b1, S_BT2);
				end

				S_BT2: begin
					if (wrapped) state <= S_DONE;
					else seek(1'b1, 1'b1, S_BT3);
				end

				S_BT3: begin
					if (wrapped) state <= S_DONE;
					else seek(1'b0, 1'b0, S_DONE);
				end

				// ---- ESC 1 and ESC 2 in protect mode: the protect bit of the
				// column left of the cursor, from the cursor's line to the last.
				// The step to the left is taken against the limit some other
				// routine left behind: if the cursor happens to be on it the
				// cursor's own column is taken.  Behind row 23 of the memory the
				// walk goes on in row 0, and from column 0 it is then beside the
				// screen.
				S_CP0: begin
					pr    <= cur_r;
					pc    <= cur_c;
					back  <= 1'b1;
					state <= S_CP1;
				end

				S_CP1: begin
					if (!at_limit) begin
						pr <= b_r;
						pc <= b_c;
					end
					state <= S_CP2;
				end

				S_CP2: begin
					el <= line_of(pr, bot);
					after1(S_CP3);
				end

				S_CP3: begin
					wdata_r <= {4'd0, cp_set, rdata[6:0]};
					we_c    <= ~poff;
					if (el == last_line) begin
						pr <= 5'd0;
						if (cur_c == 7'd0) poff <= 1'b1;
						else pc <= cur_c - 7'd1;
					end else pr <= pr + 5'd1;
					el <= el + 5'd1;
					if (el == 5'd23) begin
						poff  <= 1'b0;
						state <= S_DONE;
					end else after2(S_CP3);
				end

				// ---- ESC f: the protect bit, and in write attribute mode the
				// attributes, of every cell from the one to the other; the
				// characters stay.
				S_SF0: begin
					if (!f_ok) state <= S_DONE;
					else begin
						pr   <= row_of(f_r1[4:0], bot);
						pc   <= f_c1;
						lr   <= row_of(f_r2[4:0], bot);
						lc   <= f_c2;
						back <= 1'b0;
						after2(S_SF1);
					end
				end

				S_SF1: begin
					wdata_r <= {at, m_wpt, rdata[6:0]};
					we_c    <= 1'b1;
					we_a    <= m_atb;
					if (at_limit) seek(1'b0, 1'b0, S_DONE);
					else begin
						pr <= f_r;
						pc <= f_c;
						after2(S_SF1);
					end
				end

				// ---- erasing.  From the cursor, or from the first line, to the
				// end of the page.  If protected cells are not to be kept
				// everything goes: characters, protect bits, attributes, and the
				// attributes that are switched on are switched off.  If they are
				// to be kept they are kept in protect mode only, and the cells
				// are written like characters.
				S_ERA0: begin
					if (er_whole) begin
						cur_c <= 7'd0;
						cur_r <= home_r;
					end
					state <= S_ERA1;
				end

				S_ERA1: begin
					pr     <= cur_r;
					pc     <= cur_c;
					el     <= cur_line;
					er_all <= ~tabs[72];
					if (!tabs[72]) raw <= 1'b1;
					else tabs[79:72] <= tabs[79:72] & {8{m_prt}};
					call(S_EOL, S_ERAR);
				end

				S_ERAR: begin
					pr <= (el == last_line) ? 5'd0 : pr + 5'd1;
					pc <= 7'd0;
					el <= el + 5'd1;
					if (el == 5'd23) state <= S_ERA3;
					else call(S_EOL, S_ERAR);
				end

				S_ERA3: begin
					raw <= 1'b0;
					if (er_all) begin
						at    <= 4'd0;
						dirty <= 1'b1;
					end
					sk_want <= 1'b0;
					sk_back <= 1'b0;
					state   <= S_SEEK;
				end

				// from the pointer to the end of its line
				S_EOL: begin
					if (tabs[72]) after1(S_EOLT);
					else begin
						put(fill_sp ? 7'h20 : 7'h00);
						pc <= pc + 7'd1;
						if (pc == 7'd79) leave;
					end
				end

				S_EOLT: begin
					if (!rdata[7]) put(fill_sp ? 7'h20 : 7'h00);
					pc <= pc + 7'd1;
					if (pc == 7'd79) leave;
					else after2(S_EOLT);
				end

				S_EOT: seek(1'b0, 1'b0, S_DONE);

				// ---- ESC X: every page.  The search at the end of clearing a
				// page can leave another page current, and the count goes on
				// from that one: back to an earlier page means round and round
				// for ever.
				S_CLA0: begin
					fill_sp     <= 1'b0;
					tabs[79:72] <= 8'hFF;
					sv_pg       <= pg;
					pg          <= 1'b0;
					rounds      <= 5'd0;
					state       <= S_CLA1;
				end

				S_CLA1: begin
					if (rounds == 5'd16) begin
						hung  <= 1'b1;
						state <= S_IDLE;
					end else begin
						rounds   <= rounds + 5'd1;
						cpu      <= pg;
						raw      <= 1'b1;
						er_whole <= 1'b1;
						call(S_ERA0, S_CLA2);
					end
				end

				S_CLA2: begin
					if (!TWO || pg) state <= S_CLA3;
					else begin
						pg    <= 1'b1;
						state <= S_CLA1;
					end
				end

				S_CLA3: begin
					pg    <= sv_pg;
					cpu   <= sv_pg;
					at    <= 4'd0;
					dirty <= 1'b1;
					state <= S_CLA4;
				end

				S_CLA4: begin
					cur_c <= 7'd0;
					cur_r <= home_r;
					seek(1'b0, 1'b0, S_DONE);
				end

				// ---- ESC Q and ESC W: a character inserted or deleted, to the
				// end of the line, in protect mode to the end of the field.  A
				// cell moves with its protect bit and its attributes; the blank
				// is written like a character.
				S_ID0: begin
					pr     <= cur_r;
					pc     <= cur_c;
					id_end <= 7'd80;
					if (in_status) state <= S_DONE;
					else if (m_prt) after2(S_IDFE);
					else state <= S_ID1;
				end

				// the column of the first protected cell from the cursor on
				S_IDFE: begin
					if (rdata[7]) begin
						id_end <= pc;
						state  <= S_ID1;
					end else if (pc == 7'd79) state <= S_ID1;
					else begin
						pc <= pc + 7'd1;
						after2(S_IDFE);
					end
				end

				S_ID1: begin
					id_cnt <= id_end - cur_c;
					pc     <= id_ins ? id_end - 7'd2 : cur_c + 7'd1;
					state  <= (id_end == cur_c) ? S_DONE : S_ID2;
				end

				S_ID2: begin
					if (id_cnt == 7'd1) state <= S_IDB;
					else after1(S_ID3);
				end

				S_ID3: begin
					hold  <= rdata;
					pc    <= id_ins ? pc + 7'd1 : pc - 7'd1;
					state <= S_ID4;
				end

				S_ID4: begin
					put_all(hold);
					pc     <= id_ins ? pc - 7'd2 : pc + 7'd2;
					id_cnt <= id_cnt - 7'd1;
					state  <= S_ID2;
				end

				S_IDB: begin
					pc    <= id_ins ? cur_c : id_end - 7'd1;
					state <= S_IDB2;
				end

				S_IDB2: begin
					put(7'h20);
					seek(1'b0, 1'b0, S_DONE);
				end

				// ---- ESC E: a line inserted.  Not in protect mode.  The cursor
				// stays where it is.  With the cursor in the upper half of the
				// screen the page is rolled and the lines above the cursor are
				// moved; in the lower half the lines below.  With auto flip the
				// last line of the page goes to the top of the next.  The new
				// line is blanks over a copy of the line that was the cursor's:
				// it has that line's attributes.
				S_IL0: begin
					if (m_prt) state <= S_DONE;
					else if (chain) begin
						bot1 <= (bot1 == 5'd0) ? 5'd23 : bot1 - 5'd1;
						copy(1'b0, 5'd23, 1'b1, 5'd0, S_IL1);
					end else state <= S_IL1;
				end

				S_IL1: begin
					lrow  <= cur_line;
					state <= S_IL2;
				end

				S_IL2: begin
					if (lrow < 5'd12) begin
						cur_r <= (cur_r == 5'd0) ? 5'd23 : cur_r - 5'd1;
						if (pg) bot1 <= (bot1 == 5'd0) ? 5'd23 : bot1 - 5'd1;
						else bot0 <= (bot0 == 5'd0) ? 5'd23 : bot0 - 5'd1;
						ln    <= 5'd1;
						state <= S_IL3;
					end else begin
						ln    <= 5'd22;
						state <= S_IL5;
					end
				end

				S_IL3: begin
					ln <= ln + 5'd1;
					if (ln > lrow) state <= S_IL4;
					else copy(pg, ln, pg, ln - 5'd1, S_IL3);
				end

				S_IL4: copy(pg, lrow + 5'd1, pg, lrow, S_IL7);

				S_IL5: begin
					ln <= ln - 5'd1;
					if (ln < lrow) state <= S_IL7;
					else copy(pg, ln, pg, ln + 5'd1, S_IL5);
				end

				S_IL7: begin
					pr <= cur_r;
					pc <= 7'd0;
					call(S_FILL, S_DONE);
				end

				// ---- ESC R: a line deleted.  Not in protect mode.  With auto
				// flip the first line of the next page comes up to the end of
				// this one.  The line that comes free is blanks over a copy of
				// the line above it.
				S_DL0: begin
					lrow  <= cur_line;
					state <= m_prt ? S_DONE : S_DL1;
				end

				S_DL1: begin
					if (lrow < 5'd12) begin
						ln    <= lrow;
						state <= S_DL2;
					end else begin
						ln    <= lrow + 5'd1;
						state <= S_DL4;
					end
				end

				S_DL2: begin
					ln <= ln - 5'd1;
					if (ln == 5'd0) state <= S_DL3;
					else copy(pg, ln - 5'd1, pg, ln, S_DL2);
				end

				S_DL3: begin
					cur_r <= (cur_r == 5'd23) ? 5'd0 : cur_r + 5'd1;
					if (pg) bot1 <= (bot1 == 5'd23) ? 5'd0 : bot1 + 5'd1;
					else bot0 <= (bot0 == 5'd23) ? 5'd0 : bot0 + 5'd1;
					state <= S_DL5;
				end

				S_DL4: begin
					ln <= ln + 5'd1;
					if (ln == 5'd24) state <= S_DL5;
					else copy(pg, ln, pg, ln - 5'd1, S_DL4);
				end

				S_DL5: begin
					if (chain) copy(1'b1, 5'd0, 1'b0, 5'd23, S_DL6);
					else state <= S_DL7;
				end

				S_DL6: begin
					bot1  <= (bot1 == 5'd23) ? 5'd0 : bot1 + 5'd1;
					state <= S_DL7;
				end

				S_DL7: copy(pg_last, 5'd22, pg_last, 5'd23, S_DL8);

				S_DL8: begin
					cpu <= pg_last;
					pr  <= pg_last ? bot1 : bot0;
					pc  <= 7'd0;
					call(S_FILL, S_DL9);
				end

				S_DL9: begin
					cpu   <= pg;
					state <= S_DONE;
				end

				// a line to another, of the same or another page, cells whole
				S_CPR0: begin
					sm    <= row_of(cp_sl, cp_sp ? bot1 : bot0);
					dm    <= row_of(cp_dl, cp_dp ? bot1 : bot0);
					state <= S_CPR1;
				end

				S_CPR1: begin
					cpu   <= cp_sp;
					pr    <= sm;
					pc    <= 7'd0;
					state <= S_CPRR;
				end

				// A cell is read and the one before it is written, in turn: what
				// is read here is the cell the pointer was at two steps ago.
				S_CPRR: begin
					hold  <= rdata;
					cpu   <= cp_dp;
					pr    <= dm;
					pc    <= pc - 7'd1;
					state <= S_CPRW;
				end

				S_CPRW: begin
					if (pc != 7'd127) put_all(hold);
					cpu <= cp_sp;
					pr  <= sm;
					pc  <= pc + 7'd2;
					if (pc == 7'd79) begin
						cpu <= pg;
						leave;
					end else state <= S_CPRR;
				end

				// ---- ESC 4 to ESC 7: from the start of the cursor's line, or
				// from the first line, up to the cursor and with it.  Empty cells
				// are not sent, but in program mode; US is sent at the end of
				// each line and CR at the end, not in program mode.
				S_SND0: begin
					pr   <= snd_home ? home_r : cur_r;
					pc   <= 7'd0;
					er   <= cur_r;
					ec   <= cur_c;
					s_at <= 4'd0;
					s_pr <= 1'b0;
					after2(S_SND1);
				end

				S_SND1: begin
					hold  <= rdata;
					state <= S_SNDA;
				end

				// in write attribute mode, where the attributes change: ESC and
				// the letter that switches each one on or off
				S_SNDA: begin
					achg <= s_at ^ hold[11:8];
					awas <= s_at;
					ai   <= 2'd0;
					if (m_atb && hold[11:8] != s_at) begin
						s_at  <= hold[11:8];
						state <= S_SNDA1;
					end else state <= S_SNDP;
				end

				S_SNDA1: begin
					if (achg[0]) emit(ESC, S_SNDA2);
					else state <= S_SNDA3;
				end

				S_SNDA2: emit({at_letter, awas[0]}, S_SNDA3);

				S_SNDA3: begin
					achg  <= {1'b0, achg[3:1]};
					awas  <= {1'b0, awas[3:1]};
					ai    <= ai + 2'd1;
					state <= (achg[3:1] == 3'd0) ? S_SNDP : S_SNDA1;
				end

				// In protect mode, where protected cells begin or end: ESC ) and
				// ESC ( if all is sent; else FS, and the protected cells are
				// walked over.  That walk goes the way the last walk of anything
				// went: backward, if the cursor was last moved backward.
				S_SNDP: begin
					if (m_prt && hold[7] != s_pr) begin
						if (send_all) emit(ESC, S_SNDP1);
						else emit(7'h1C, S_SNDQ);
					end else state <= S_SNDC;
				end

				S_SNDP1: begin
					s_pr <= hold[7];
					emit(s_pr ? 7'h28 : 7'h29, S_SNDC);
				end

				S_SNDQ: begin
					if (!hold[7]) begin
						s_pr  <= 1'b0;
						state <= S_SNDC;
					end else if (at_end) state <= S_SNDC;
					else begin
						pr <= n_r;
						pc <= n_c;
						if (n_wrap) emit(7'h1F, S_SNDQ1);
						else state <= S_SNDQ1;
					end
				end

				S_SNDQ1: after1(S_SNDQ2);

				S_SNDQ2: begin
					hold  <= rdata;
					state <= S_SNDQ;
				end

				S_SNDC: begin
					if (m_pgm || hold[6:0] != 7'd0) emit(pictured(hold[6:0]), S_SNDD);
					else state <= S_SNDD;
				end

				S_SNDD: begin
					back <= 1'b0;
					if (at_end) begin
						if (!m_pgm) emit(7'h0D, S_SNDE);
						else state <= S_SNDE;
					end else begin
						pr <= f_r;
						pc <= f_c;
						if (f_wrap) emit(7'h1F, S_SNDR);
						else state <= S_SNDR;
					end
				end

				S_SNDR: after1(S_SND1);

				S_SNDE: begin
					sending <= 1'b0;
					state   <= S_IDLE;
				end

				// ---- the status line
				S_ST0: begin
					st_col  <= 7'd0;
					st_mask <= {m_pgm, m_prt, m_blk, m_wpt, m_atb, m_flp};
					st_am   <= at;
					st_fdx  <= 1'b0;
					a_st    <= 1'b1;
					state   <= S_ST1;
				end

				S_ST1: begin
					wdata_r <= {1'b0, st_fl, 2'b00, st_ch};
					we_c    <= st_we;
					we_a    <= st_we;
					if (st_col[6:2] == 5'd5) st_am <= st_am & (st_am - 4'd1);
					if (st_col >= 7'd30 && st_ph == 2'd3) begin
						if (st_mask == 6'd0) st_fdx <= 1'b1;
						st_mask <= st_mask & (st_mask - 6'd1);
					end
					st_col <= st_col + 7'd1;
					if (st_col == 7'd79) state <= S_ST2;
				end

				S_ST2: begin
					a_st    <= 1'b0;
					st_init <= 1'b0;
					if (st_init) state <= S_ST0;
					else begin
						dirty <= 1'b0;
						err   <= 3'd0;
						state <= sending ? S_SND0 : S_IDLE;
					end
				end

				// ---- a character for the computer waits to be taken
				S_TX: begin
					if (dout_ready) begin
						dout_valid <= 1'b0;
						state      <= tx_ret;
					end
				end

				// ---- the memory takes two clocks
				S_W2: state <= S_W1;
				S_W1: state <= ret;

				default: state <= S_IDLE;
			endcase
	end

	// the bell sounds for eight frames
	reg [3:0] bell_n;
	assign bell = (bell_n != 4'd0);
	always @(posedge clk)
		if (reset) bell_n <= 4'd0;
		else if (ring) bell_n <= 4'd8;
		else if (frame && (bell_n != 4'd0)) bell_n <= bell_n - 4'd1;

	// what the display needs
	wire [4:0] vis_bot = vis ? bot1 : bot0;
	always @(posedge clk) top_row <= (vis_bot == 5'd23) ? 5'd0 : vis_bot + 5'd1;
	assign page       = vis;
	assign cur_x      = cur_c;
	assign cur_y      = cur_r;
	assign cur_status = in_status;
	assign prog_mode  = m_pgm;

endmodule
