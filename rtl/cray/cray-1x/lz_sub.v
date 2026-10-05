module lz_sub(i_data, z_bar, o_zeros);

    input wire [7:0] i_data;
    output wire z_bar;
    output reg [2:0] o_zeros;


    assign z_bar =|i_data[7:0];

    always@*
        begin
            casex(i_data)
                8'b1xxxxxxx:o_zeros[2:0]=3'b000;
                8'b01xxxxxx:o_zeros[2:0]=3'b001;
                8'b001xxxxx:o_zeros[2:0]=3'b010;
                8'b0001xxxx:o_zeros[2:0]=3'b011;
                8'b00001xxx:o_zeros[2:0]=3'b100;     
                8'b000001xx:o_zeros[2:0]=3'b101;
                8'b0000001x:o_zeros[2:0]=3'b110;
                8'b00000001:o_zeros[2:0]=3'b111;
                8'b00000000:o_zeros[2:0]=3'b000;
            endcase
        end


endmodule
