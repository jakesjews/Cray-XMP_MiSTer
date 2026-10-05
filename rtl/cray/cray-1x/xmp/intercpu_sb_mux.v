module intercpu_sb_mux(i_cln, i_j, i_sb_1, i_sb_2, i_sb_3, i_sb_4, i_sb_5, o_ai);

input wire [2:0] i_cln;
input wire [2:0] i_j;
input wire [191:0] i_sb_1, i_sb_2, i_sb_3, i_sb_4, i_sb_5;
output reg [23:0] o_ai;

always@*
   begin
	   case(i_cln)
			3'h0:o_ai = 24'b0;
			3'h1:case(i_j[2:0])
			        3'h0:o_ai = i_sb_1[23:0];
					  3'h1:o_ai = i_sb_1[47:24];
					  3'h2:o_ai = i_sb_1[71:48];
					  3'h3:o_ai = i_sb_1[95:72];
					  3'h4:o_ai = i_sb_1[119:96];
					  3'h5:o_ai = i_sb_1[143:120];
					  3'h6:o_ai = i_sb_1[167:144];
					  3'h7:o_ai = i_sb_1[191:168];
			     endcase 
			3'h2:case(i_j[2:0])
			        3'h0:o_ai = i_sb_2[23:0];
					  3'h1:o_ai = i_sb_2[47:24];
					  3'h2:o_ai = i_sb_2[71:48];
					  3'h3:o_ai = i_sb_2[95:72];
					  3'h4:o_ai = i_sb_2[119:96];
					  3'h5:o_ai = i_sb_2[143:120];
					  3'h6:o_ai = i_sb_2[167:144];
					  3'h7:o_ai = i_sb_2[191:168];
			     endcase 
			3'h3:case(i_j[2:0])
			        3'h0:o_ai = i_sb_3[23:0];
					  3'h1:o_ai = i_sb_3[47:24];
					  3'h2:o_ai = i_sb_3[71:48];
					  3'h3:o_ai = i_sb_3[95:72];
					  3'h4:o_ai = i_sb_3[119:96];
					  3'h5:o_ai = i_sb_3[143:120];
					  3'h6:o_ai = i_sb_3[167:144];
					  3'h7:o_ai = i_sb_3[191:168];
			     endcase 
			3'h4:case(i_j[2:0])
			        3'h0:o_ai = i_sb_4[23:0];
					  3'h1:o_ai = i_sb_4[47:24];
					  3'h2:o_ai = i_sb_4[71:48];
					  3'h3:o_ai = i_sb_4[95:72];
					  3'h4:o_ai = i_sb_4[119:96];
					  3'h5:o_ai = i_sb_4[143:120];
					  3'h6:o_ai = i_sb_4[167:144];
					  3'h7:o_ai = i_sb_4[191:168];
			     endcase 
			3'h5:case(i_j[2:0])
			        3'h0:o_ai = i_sb_5[23:0];
					  3'h1:o_ai = i_sb_5[47:24];
					  3'h2:o_ai = i_sb_5[71:48];
					  3'h3:o_ai = i_sb_5[95:72];
					  3'h4:o_ai = i_sb_5[119:96];
					  3'h5:o_ai = i_sb_5[143:120];
					  3'h6:o_ai = i_sb_5[167:144];
					  3'h7:o_ai = i_sb_5[191:168];
			     endcase 
			default:o_ai = 24'b0;
		endcase
	end

endmodule