derive_pll_clocks
derive_clock_uncertainty

# core specific constraints

# The machine's clock (PLL output 1) and the video clock (output 0) only meet in
# the synchronisers of rtl/mister/cdc.v.
set_clock_groups -asynchronous \
	-group [get_clocks {*|pll|pll_inst|altera_pll_i|general[0].gpll~PLL_OUTPUT_COUNTER|divclk}] \
	-group [get_clocks {*|pll|pll_inst|altera_pll_i|general[1].gpll~PLL_OUTPUT_COUNTER|divclk}]
