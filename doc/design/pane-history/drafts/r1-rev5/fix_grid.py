import sys,re,html,unicodedata
src,dst=sys.argv[1],sys.argv[2]
s=open(src,encoding='utf-8').read()
def cw(ch):
    return 2 if unicodedata.east_asian_width(ch) in 'WF' else 1
def cellify(text):
    out=[]
    for ch in html.unescape(text):
        e=html.escape(ch,quote=False)
        if ch==' ': out.append(' ')
        elif ord(ch)<128: out.append(e)
        else: out.append(f'<i class="{"w" if cw(ch)==2 else "n"}">{e}</i>')
    return ''.join(out)
def fix_pre(m):
    open_tag,body=m.group(1),m.group(2)
    # split into tags and text; cellify text
    parts=re.split(r'(<[^>]+>)',body)
    body=''.join(p if p.startswith('<') else cellify(p) for p in parts)
    # wrap each line in span.ln
    lines=body.split('\n')
    # tags may span lines: close/reopen open spans per line
    res=[];stack=[]
    for ln in lines:
        prefix=''.join(stack)
        for t in re.findall(r'<(/?)span([^>]*)>',ln):
            if t[0]=='': stack.append(f'<span{t[1]}>')
            else: stack and stack.pop()
        res.append('<span class="ln">'+prefix+ln+'</span>'*len(stack)+'</span>')
    return open_tag+''.join(res)+'</pre>'
s=re.sub(r'(<pre class="grid[^"]*"[^>]*>)(.*?)</pre>',fix_pre,s,flags=re.S)
css='''
pre.grid{font-family:Menlo,Consolas,"DejaVu Sans Mono","Liberation Mono",monospace;line-height:1.2}
pre.grid .ln{display:block;white-space:pre}
pre.grid i{font-style:normal;display:inline-block;text-align:center;vertical-align:top}
pre.grid i.w{font-family:"PingFang TC","Microsoft JhengHei","Noto Sans CJK TC","WenQuanYi Zen Hei",sans-serif}
pre.grid i.n{width:1ch}
pre.grid i.w{width:2ch}
'''
s=s.replace('</style>',css+'</style>',1)
# artifact theme guards
s=s.replace('@media (prefers-color-scheme:dark){:root{','@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){',1)
s=s.replace('</style>',':root[data-theme="dark"]{--pg:#17171a;--fg:#e8e8e8;--mut:#a8a8a8;--line:#3a3a40;--card:#202024}\n</style>',1)
open(dst,'w',encoding='utf-8').write(s)
