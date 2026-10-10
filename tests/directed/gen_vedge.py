#!/usr/bin/env python3
"""Write vedge.cal: vector loads that run off the end of a user's field, at
every step the memory unit reads as parts of lines.

    python3 tests/directed/gen_vedge.py > tests/directed/vedge.cal

The user's field is 20000 octal words; its last 1000 hold their own
relative addresses.  Each load of 64 elements starts so that k of them lie
inside the field and the rest beyond it, for steps 1 to 9, and a few load
downwards or with a step of 0.  The interrupt on operand range is off, so
the program goes on; every register loaded is stored into a slot of its own
behind the program, where the elements from outside the field must be zero.  The reference model predicts the slots and the flags of the
package.
"""
STEPS = [1, 2, 3, 4, 5, 6, 7, 8, 9]
INSIDE = [1, 2, 3, 16, 17, 40, 63]
FIELD = 0o20000
SLOTS = 0o1000     # the slots start here, behind the program

lines = []
e = lines.append
e("* vedge: vector loads off the end of a user's field (see gen_vedge.py).")
e("         INCLUDE \"rt_direct.cal\"")
e("         INCLUDE \"rt_user.cal\"")
e("UPKG     =       3000")
e("UBASE    =       20000                   ; the user's field: 20000 to 40000")
e("         TBEGIN")
e("         S1      KFI,0")
e("         U$W1,0  S1")
e("         S1      KFL,0")
e("         U$W2,0  S1")
e("         S1      KFD,0")
e("         U$W4,0  S1")
e("         S1      KFE,0")
e("         U$W5,0  S1")
e("* the last 1000 words of the field hold their relative addresses")
e("         A1      17000")
e("         A2      20000")
e("FILL     S1      A1")
e("         UBASE,A1 S1")
e("         A1      A1+1")
e("         A0      A2-A1")
e("         JAN     FILL")
e("         UGO     KPA")
e("         TPASS")
e("         TEND")
e("KFI      CON     UBASE/40*T$P2_29        ; IBA")
e("KFL      CON     (UBASE+20000)/40*T$P2_29 ; ILA, no interrupt on operand range")
e("KFD      CON     UBASE/40*T$P2_29        ; DBA")
e("KFE      CON     (UBASE+20000)/40*T$P2_29 ; DLA")
e("KPA      CON     (UA-UBASE*4)*T$P2_24    ; P of the user program, relative to the base")
e("         UDATA")
e("*")
e("* the user program; its addresses are relative to UBASE")
e("         ORG     UBASE+10")
e("UA       A1      D'64")
e("         VL      A1")
slot = 0
cases = [(s, k) for s in STEPS for k in INSIDE]
for s, k in cases:
    start = FIELD - k * s
    v = slot % 7 + 1
    e("* step %d, %d elements inside" % (s, k))
    e("         A2      D'%d" % s)
    e("         A0      O'%o" % start)
    e("         V%d      ,A0,A2" % v)
    e("         A0      O'%o" % (SLOTS + slot * 0o100))
    e("         ,A0,1   V%d" % v)
    slot += 1
# downwards from near the start of the field, and a step of 0 at its end
for s, start in [(-1, 5), (-3, 0o100), (0, 0o17777)]:
    v = slot % 7 + 1
    e("* step %d from %o" % (s, start))
    e("         A2      D'%d" % abs(s))
    if s < 0:
        e("         A3      0")
        e("         A2      A3-A2")
    e("         A0      O'%o" % start)
    e("         V%d      ,A0,A2" % v)
    e("         A0      O'%o" % (SLOTS + slot * 0o100))
    e("         ,A0,1   V%d" % v)
    slot += 1
assert SLOTS + slot * 0o100 <= 0o17000
e("* and a short one off the end")
e("         A1      5")
e("         VL      A1")
e("         A2      3")
e("         A0      O'17772")
e("         V1      ,A0,A2")
e("         A0      O'%o" % (SLOTS + slot * 0o100))
e("         ,A0,1   V1")
e("         EX")
e("         END")
print("\n".join(lines))
