//******************************************
//       ADDRESS ADD UNIT
//******************************************
//The address add unit performs 24-bit integer addition and subtraction. The unit
//executes instructions 030 and 031. The addition and subtraction are performed in
//a similar manner. However, the two's complement subtraction for the 031 instruction
// occurs as follows. The one's compelement of the Ak operand is added to the Aj operand.
// Then a one is added in the low order position of the result. 
//No overflow is detected in the functional unit.
//The functional unit time is two clock periods


module addr_add(i_ak,i_aj,i_instr,clk,o_result);

input wire [23:0] i_ak;       
input wire [23:0] i_aj;            

output reg [23:0] o_result;
input wire clk;
input wire [6:0] i_instr;

reg [23:0] ak, aj;
reg [6:0] instr;
//now compute the result   
always@(posedge clk) begin 
   ak <= i_ak;
   aj <= i_aj;
   instr <= i_instr;
   o_result[23:0] <= (instr[6:0]==7'b0011000) ? (ak[23:0] + aj[23:0]) : (aj[23:0] + ~ak[23:0] + 24'h000001); 
end

endmodule
