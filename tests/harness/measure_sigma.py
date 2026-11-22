import sys, glob, math
W,H=640,360
def sigma(path):
    a=open(path,'rb').read()
    # alpha edge at x=160 on rows outside the half-transparent stripe; average rows 60..120
    prof=[0.0]*W
    rows=range(150,211)
    for y in rows:
        for x in range(W): prof[x]+=a[y*W+x]/255.0
    prof=[p/len(rows) for p in prof]
    seg=prof[0:640]
    # 10-90 width
    x10=next(i for i,v in enumerate(seg) if v>=0.1)
    x90=next(i for i,v in enumerate(seg) if v>=0.9)
    # sub-pixel interpolation
    def cross(t):
        i=next(i for i,v in enumerate(seg) if v>=t)
        v0,v1=seg[i-1],seg[i]
        return i-1+(t-v0)/(v1-v0) if v1!=v0 else i
    w=cross(0.9)-cross(0.1)
    return w/2.563
for p in sorted(glob.glob(sys.argv[1]+'/*.alpha')):
    print(f"{p.split('/')[-1][:-10]:24s} sigma={sigma(p):6.2f}")
