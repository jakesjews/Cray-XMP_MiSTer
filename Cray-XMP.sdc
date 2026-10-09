derive_pll_clocks
derive_clock_uncertainty

# core specific constraints

# The CPU's clock, the I/O Subsystem's and the video clock, each from a PLL of
# its own, only meet in synchronisers: those of rtl/xmp_bridge.v between the
# first two, those of rtl/mister/cdc.v between either and the third.
set_clock_groups -asynchronous \
	-group [get_clocks {*|pll|pll_inst|altera_pll_i|general[0].gpll~PLL_OUTPUT_COUNTER|divclk}] \
	-group [get_clocks {*|pll_cpu|altera_pll_i|general[0].gpll~PLL_OUTPUT_COUNTER|divclk}] \
	-group [get_clocks {*|pll_ios|altera_pll_i|general[0].gpll~PLL_OUTPUT_COUNTER|divclk}]

# The framework sets the core's first PLL apart from its own clocks by name
# (sys/sys_top.sdc); the other two need the same said here.  A group by itself
# is apart from every other clock.
set_clock_groups -asynchronous \
	-group [get_clocks {*|pll_ios|altera_pll_i|general[0].gpll~PLL_OUTPUT_COUNTER|divclk}]
set_clock_groups -asynchronous \
	-group [get_clocks {*|pll_cpu|altera_pll_i|general[0].gpll~PLL_OUTPUT_COUNTER|divclk}]
