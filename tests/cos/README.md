# Loops that time memory under COS

Eight programs for the assembler on the drive. Each runs a loop over 4,096
words 1,000 times and ends; the CPU time COS charges the job step, divided
by 4,096,000 and by the clock period of 9.5 ns, is the clock periods the
loop takes for each element. `docs/MACHINE.md` has what they gave on a
DE10-Nano, under "Differences from a real X-MP".

- `sno.cal`: the scalar loop with nothing in it
- `sld.cal`, `sst.cal`: with a scalar load, with a scalar store
- `vld.cal`, `vst.cal`: a vector load, a vector store, 64 words at a time
- `vrg.cal`: a vector add of registers, no memory
- `vec.cal`, `sca.cal`: two loads, an add and a store, with vector and with
  scalar instructions

`tools/py/membench.py` runs the same loops in the CPU simulation
(`tests/bench/membench.cal`), for a look before a build.

To run one, put it on the expander disk as a job behind these statements
(`tools/py/expdisk.py puttext`, see `docs/DEVELOPMENT.md`) and submit it at
the station. The log printed with the job has the CPU time at `$ABD.` and
at `END OF JOB`.

```
JOB,JN=JVEC,T=300.
ACCOUNT,AC=CRAY,US=SYSTEM.
COPYF,O=SRC.
REWIND,DN=SRC.
MEMORY,FL,USER.
CAL,I=SRC,L=0.
LDR,AB,DN=$BLD.
$ABD.
/EOF
```
