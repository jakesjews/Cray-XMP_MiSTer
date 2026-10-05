#!/usr/bin/env python3
"""Bring-up test for the vector instructions; expected values are computed here."""
import json, random
random.seed(11)
M=(1<<64)-1
def rows(name):
    return [l.split() for l in open('build/fpvec/%s.vec'%name) if l.strip() and not l.startswith('#')]

N=64
va=[random.getrandbits(64) for _ in range(N)]
vb=[random.getrandbits(64) for _ in range(N)]
va[3]=0; va[10]=1<<63; va[20]=0; vb[5]=0
for i in range(0,N,7): va[i]=random.getrandbits(8)
sc=0x0123456789ABCDEF
vm=random.getrandbits(64)

IN_A=0o20000; IN_B=0o20100; OUT=0o30000
code=[]; exp={}; slot=[0]
def emit(*l): code.extend(l)
def store(reg, vals, vl):
    base=OUT+slot[0]*0o100; slot[0]+=1
    emit("         A0        O'%o"%base, "         ,A0,1     V%d"%reg)
    for i in range(vl): exp[base+i]=vals[i]&M
    return base
def setvl(n):
    emit("         A1        D'%d"%n, "         VL        A1")

hdr=["         IDENT     TV","TEXIT    =         O'3777762","         ORG       0","         CON       P.START*O'100000000","         CON       0","         CON       O'1777776100000000","         CON       O'0000000030000000000000","         BSSZ      D'12","         ORG       O'40"]
emit("START    A1        D'64","         VL        A1","         A0        O'%o"%IN_A,"         V1        ,A0,1","         A0        O'%o"%IN_B,"         V2        ,A0,1")
emit("         S1        SC,0","         S2        VMV,0")
store(1,va,64); store(2,vb,64)                                   # load then store round trip
for vl in (64,5,1,63):
    setvl(vl)
    emit("         V3        V1+V2"); store(3,[a+b for a,b in zip(va,vb)],vl)
    emit("         V3        S1+V2"); store(3,[sc+b for b in vb],vl)
    emit("         V4        V1-V2"); store(4,[a-b for a,b in zip(va,vb)],vl)
    emit("         V4        S1-V2"); store(4,[sc-b for b in vb],vl)
    emit("         V5        V1&V2"); store(5,[a&b for a,b in zip(va,vb)],vl)
    emit("         V5        S1&V2"); store(5,[sc&b for b in vb],vl)
    emit("         V6        V1!V2"); store(6,[a|b for a,b in zip(va,vb)],vl)
    emit("         V6        V1\\V2"); store(6,[a^b for a,b in zip(va,vb)],vl)
setvl(64)
# merges under a mask loaded by 003: mask bit 63-n belongs to element n
emit("         VM        S2")
bit=lambda n: (vm>>(63-n))&1
emit("         V7        V1!V2&VM"); store(7,[a if bit(i) else b for i,(a,b) in enumerate(zip(va,vb))],64)
emit("         V7        S1!V2&VM"); store(7,[sc if bit(i) else b for i,b in enumerate(vb)],64)
# 175 tests, mask read back with 073
tests={'Z':lambda x:x==0,'N':lambda x:x!=0,'P':lambda x:(x>>63)==0,'M':lambda x:(x>>63)==1}
mslot=OUT+0o7000
for vl in (64,20):
    setvl(vl)
    for t,f in tests.items():
        emit("         VM        V1,%s"%t, "         S3        VM", "         O'%o,0   S3"%mslot)
        m=0
        for i in range(vl):
            if f(va[i]): m|=1<<(63-i)
        exp[mslot]=m; mslot+=1
setvl(64)
# shifts
for cnt in (0,1,3,63,64,65,127,128):
    emit("         A2        D'%d"%cnt)
    emit("         V3        V1<A2"); store(3,[(a<<cnt)&M if cnt<64 else 0 for a in va],64)
    emit("         V3        V1>A2"); store(3,[(a>>cnt) if cnt<64 else 0 for a in va],64)
    for vl in (64,4,1):
        setvl(vl)
        dl=[]; dr=[]
        for n in range(vl):
            nxt=va[n+1] if n+1<vl else 0
            dl.append((((va[n]<<64)|nxt)<<cnt>>64)&M if cnt<128 else 0)
            prv=va[n-1] if n>0 else 0
            dr.append((((prv<<64)|va[n])>>cnt)&M if cnt<128 else 0)
        emit("         V4        V1,V1<A2"); store(4,dl,vl)
        emit("         V4        V1,V1>A2"); store(4,dr,vl)
    setvl(64)
# element access 076 / 077
emit("         A3        D'17","         S4        V1,A3","         O'%o,0   S4"%mslot); exp[mslot]=va[17]; mslot+=1
emit("         A3        D'40","         V5        V1!V1","         V5,A3     S1")
v5=list(va); v5[40]=sc; store(5,v5,64)
# recursive integer sum (manual page 3-16): V2 is operand and result, unit time 3 so groups of 5
emit("         A0        O'%o"%IN_B,"         V2        ,A0,1","         V2        V1+V2")
r=list(vb); D=5
for n in range(64): r[n]=(va[n]+(vb[0] if n<D else r[n-D]))&M
store(2,r,64)
# recursive logical difference clears the register: V6 = V6 \ V6
emit("         V6        V6\\V6"); store(6,[0]*64,64)
# strides: load every other word, and backwards; store with a stride
emit("         A0        O'%o"%IN_A,"         A4        2","         A1        D'32","         VL        A1","         V3        ,A0,A4"); store(3,[va[2*i] for i in range(32)],32)
emit("         A0        O'%o"%(IN_A+63),"         A4        -1","         A1        D'64","         VL        A1","         V3        ,A0,A4"); store(3,[va[63-i] for i in range(64)],64)
sbase=OUT+0o7400
emit("         A0        O'%o"%sbase,"         A4        3","         A1        D'10","         VL        A1","         ,A0,A4    V1")
for i in range(10): exp[sbase+3*i]=va[i]
setvl(0)                                                       # VL of 0 means 64
emit("         V3        V1+V2")
store(3,[a+b for a,b in zip(va,r)],64)
# floating point on vectors: operands and results from the reference vectors
fpops=[('add','V3        V1+FV2'),('sub','V3        V1-FV2'),('mul','V3        V1*FV2'),('mulh','V3        V1*HV2'),('mulr','V3        V1*RV2'),('mul2m','V3        V1*IV2'),('recip','V3        /HV1')]
fbase=0o24000
fpdata=[]
for k,(name,form) in enumerate(fpops):
    rs=rows(name); pick=random.sample(rs[:300],24)+random.sample(rs[2000:],40)
    A=[int(x[1],16) for x in pick]; B=[int(x[2],16) for x in pick]; R=[int(x[3],16) for x in pick]
    fa=fbase+k*0o200; fb=fa+0o100
    fpdata.append((fa,A)); fpdata.append((fb,B))
    emit("         A1        D'64","         VL        A1","         A0        O'%o"%fa,"         V1        ,A0,1","         A0        O'%o"%fb,"         V2        ,A0,1","         "+form)
    store(3,R,64)
# scalar-vector floating forms use (Sj)
rs=rows('mul'); pick=random.sample(rs[2000:],64); s_op=int(pick[0][1],16)
import subprocess
emit("         A6        0","         TEXIT,0   A6","HANG     J         HANG","SC       CON       O'%o"%sc,"VMV      CON       O'%o"%vm)
src=hdr+code
src+=["         ORG       O'%o"%IN_A]+["         CON       O'%o"%x for x in va]
src+=["         ORG       O'%o"%IN_B]+["         CON       O'%o"%x for x in vb]
for base,vals in fpdata:
    src+=["         ORG       O'%o"%base]+["         CON       O'%o"%x for x in vals]
src.append("         END")
open('build/t/tv.cal','w').write('\n'.join(src)+'\n')
json.dump({str(k):v for k,v in exp.items()}, open('build/t/tv.expect.json','w'))
print(len(code),'lines of code,',len(exp),'expected words,',slot[0],'stored vectors')
