import sys,os,re
d=sys.argv[1]; out=[]
for n in sorted(os.listdir(d)):
    if n.startswith('.'): continue
    L=open(os.path.join(d,n),encoding='utf-8').read().split('\n')
    exp=any('✕' in l for l in L)
    leg=next((i for i,l in enumerate(L) if '1: 1 現況' in l),None)
    ttl=next((i+1 for i,l in enumerate(L) if '✕' in l),None)
    tit=L[ttl].split('│')[-1].strip()[:6] if exp and ttl else None
    last=max((i for i,l in enumerate(L) if l.strip()),default=None)
    out.append((n,exp,ttl,leg,len(L),tit))
    print(n,'expanded' if exp else 'band-only','titlerow',ttl,'legendrow',leg,'rows',len(L),tit)
e=[o for o in out if o[1]]
print('expanded frames',len(e),'title rows',sorted({o[2] for o in e}),'legend rows',sorted({o[3] for o in e}))
